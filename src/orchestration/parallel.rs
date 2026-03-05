//! Parallel Tool Execution Framework
//!
//! This module provides sophisticated parallel execution capabilities:
//! - Dependency graph analysis for safe parallelization
//! - Resource pooling for efficient resource reuse
//! - Batched API calls to reduce overhead
//! - Execution statistics and performance tracking
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  Parallel Executor                          │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Dependency    │  │ Resource      │  │ Batch         │   │
//! │  │ Analyzer      │  │ Pool          │  │ Coordinator   │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Execution     │  │ Priority      │  │ Statistics    │   │
//! │  │ Graph         │  │ Queue         │  │ Tracker       │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, Semaphore};

use crate::tool_parser::ParsedToolCall;
use crate::tools::ToolRegistry;

/// Configuration for parallel execution
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Maximum number of concurrent tool executions
    pub max_concurrency: usize,
    /// Whether parallel execution is enabled
    pub enabled: bool,
    /// Tools that should never run in parallel (always sequential)
    pub sequential_only: HashSet<String>,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        let mut sequential_only = HashSet::new();
        // Tools that modify state and shouldn't run in parallel
        sequential_only.insert("file_write".to_string());
        sequential_only.insert("file_edit".to_string());
        sequential_only.insert("git_commit".to_string());
        sequential_only.insert("git_push".to_string());
        sequential_only.insert("shell_exec".to_string());

        Self {
            max_concurrency: 4,
            enabled: true,
            sequential_only,
        }
    }
}

/// Result of a parallel tool execution
#[derive(Debug)]
pub struct ParallelResult {
    pub tool_name: String,
    pub tool_call_id: String,
    pub result: Result<serde_json::Value>,
    pub duration_ms: u64,
}

/// Executor for parallel tool operations
pub struct ParallelExecutor {
    config: ParallelConfig,
    semaphore: Arc<Semaphore>,
}

impl ParallelExecutor {
    /// Create a new parallel executor
    pub fn new(config: ParallelConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
        Self { config, semaphore }
    }

    /// Check if a tool can run in parallel with others
    pub fn can_parallelize(&self, tool_name: &str) -> bool {
        self.config.enabled && !self.config.sequential_only.contains(tool_name)
    }

    /// Analyze tool calls and group them for execution
    /// Returns (parallel_group, sequential_group)
    pub fn analyze_calls<'a>(
        &self,
        calls: &[&'a ParsedToolCall],
    ) -> (Vec<&'a ParsedToolCall>, Vec<&'a ParsedToolCall>) {
        let mut parallel = Vec::new();
        let mut sequential = Vec::new();

        for call in calls {
            if self.can_parallelize(&call.tool_name) {
                parallel.push(*call);
            } else {
                sequential.push(*call);
            }
        }

        // If there are path conflicts, move to sequential
        let parallel = self.resolve_path_conflicts(parallel);
        let sequential_from_conflicts: Vec<_> = calls
            .iter()
            .filter(|c| {
                !parallel
                    .iter()
                    .any(|p| p.tool_name == c.tool_name && p.raw_text == c.raw_text)
            })
            .filter(|c| {
                !sequential
                    .iter()
                    .any(|s| s.tool_name == c.tool_name && s.raw_text == c.raw_text)
            })
            .copied()
            .collect();

        sequential.extend(sequential_from_conflicts);

        (parallel, sequential)
    }

    /// Remove tools from parallel group if they operate on the same paths
    fn resolve_path_conflicts<'a>(
        &self,
        calls: Vec<&'a ParsedToolCall>,
    ) -> Vec<&'a ParsedToolCall> {
        let mut seen_paths: HashSet<String> = HashSet::new();
        let mut result = Vec::new();

        for call in calls {
            if let Some(path) = extract_path(&call.arguments) {
                if seen_paths.contains(&path) {
                    // Path conflict - this will go to sequential
                    continue;
                }
                seen_paths.insert(path);
            }
            result.push(call);
        }

        result
    }

    /// Execute multiple tool calls in parallel
    pub async fn execute_parallel(
        &self,
        calls: Vec<(String, ParsedToolCall)>, // (tool_call_id, call)
        registry: Arc<ToolRegistry>,
    ) -> Vec<ParallelResult> {
        use tokio::time::Instant;

        let mut handles = Vec::new();

        for (tool_call_id, call) in calls {
            let semaphore = self.semaphore.clone();
            let registry = registry.clone();
            let tool_name = call.tool_name.clone();
            let arguments = call.arguments.clone();

            let handle = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = match semaphore.acquire().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        return ParallelResult {
                            tool_name,
                            tool_call_id,
                            result: Err(anyhow::anyhow!(
                                "Parallel execution cancelled: semaphore closed"
                            )),
                            duration_ms: 0,
                        };
                    }
                };
                let start = Instant::now();

                let result = registry.execute(&tool_name, arguments).await;
                let duration_ms = start.elapsed().as_millis() as u64;

                ParallelResult {
                    tool_name,
                    tool_call_id,
                    result,
                    duration_ms,
                }
            });

            handles.push(handle);
        }

        // Wait for all to complete
        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }

        results
    }

    /// Execute tools with automatic parallel/sequential handling
    pub async fn execute_smart(
        &self,
        calls: Vec<(String, ParsedToolCall)>,
        registry: Arc<ToolRegistry>,
    ) -> Vec<ParallelResult> {
        if !self.config.enabled || calls.len() <= 1 {
            // Single call or disabled - run sequentially
            return self.execute_sequential(calls, registry).await;
        }

        // Separate calls into parallel and sequential groups
        let parsed_calls: Vec<_> = calls.iter().map(|(_, c)| c).collect();
        let (parallel_refs, sequential_refs) = self.analyze_calls(&parsed_calls);

        // Find the actual calls with their IDs
        let parallel_calls: Vec<_> = calls
            .iter()
            .filter(|(_, c)| parallel_refs.iter().any(|p| p.raw_text == c.raw_text))
            .cloned()
            .collect();

        let sequential_calls: Vec<_> = calls
            .iter()
            .filter(|(_, c)| sequential_refs.iter().any(|s| s.raw_text == c.raw_text))
            .cloned()
            .collect();

        let mut results = Vec::new();

        // Execute parallel group first
        if !parallel_calls.is_empty() {
            let parallel_results = self
                .execute_parallel(parallel_calls, registry.clone())
                .await;
            results.extend(parallel_results);
        }

        // Then execute sequential group
        if !sequential_calls.is_empty() {
            let sequential_results = self.execute_sequential(sequential_calls, registry).await;
            results.extend(sequential_results);
        }

        results
    }

    /// Execute tools sequentially
    async fn execute_sequential(
        &self,
        calls: Vec<(String, ParsedToolCall)>,
        registry: Arc<ToolRegistry>,
    ) -> Vec<ParallelResult> {
        use tokio::time::Instant;

        let mut results = Vec::new();

        for (tool_call_id, call) in calls {
            let start = Instant::now();
            let result = registry.execute(&call.tool_name, call.arguments).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            results.push(ParallelResult {
                tool_name: call.tool_name,
                tool_call_id,
                result,
                duration_ms,
            });
        }

        results
    }
}

/// Extract path argument from tool call arguments
fn extract_path(args: &serde_json::Value) -> Option<String> {
    args.get("path")
        .or_else(|| args.get("file"))
        .or_else(|| args.get("directory"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ============================================================================
// Dependency Graph Analysis
// ============================================================================

/// A node in the dependency graph
#[derive(Debug, Clone)]
pub struct DependencyNode {
    /// Unique identifier
    pub id: String,
    /// Tool call associated with this node
    pub tool_name: String,
    /// Arguments for the tool
    pub arguments: serde_json::Value,
    /// Node IDs this depends on (must complete before this can start)
    pub depends_on: Vec<String>,
    /// Node IDs that depend on this (must wait for this to complete)
    pub dependents: Vec<String>,
    /// Priority (higher = more important)
    pub priority: u32,
    /// Execution status
    pub status: NodeStatus,
}

/// Status of a dependency node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
}

/// Dependency graph for tool execution ordering
pub struct DependencyGraph {
    nodes: HashMap<String, DependencyNode>,
    execution_order: Vec<Vec<String>>, // Levels of parallelizable nodes
}

impl DependencyGraph {
    /// Create a new dependency graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            execution_order: Vec::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, id: &str, tool_name: &str, arguments: serde_json::Value) {
        self.nodes.insert(
            id.to_string(),
            DependencyNode {
                id: id.to_string(),
                tool_name: tool_name.to_string(),
                arguments,
                depends_on: Vec::new(),
                dependents: Vec::new(),
                priority: 0,
                status: NodeStatus::Pending,
            },
        );
    }

    /// Add a dependency between nodes
    pub fn add_dependency(&mut self, from: &str, to: &str) {
        if let Some(node) = self.nodes.get_mut(to) {
            if !node.depends_on.contains(&from.to_string()) {
                node.depends_on.push(from.to_string());
            }
        }
        if let Some(node) = self.nodes.get_mut(from) {
            if !node.dependents.contains(&to.to_string()) {
                node.dependents.push(to.to_string());
            }
        }
    }

    /// Set priority for a node
    pub fn set_priority(&mut self, id: &str, priority: u32) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.priority = priority;
        }
    }

    /// Compute execution levels (nodes at same level can run in parallel)
    pub fn compute_levels(&mut self) -> Result<()> {
        self.execution_order.clear();

        let mut remaining: HashSet<String> = self.nodes.keys().cloned().collect();
        let mut completed: HashSet<String> = HashSet::new();

        while !remaining.is_empty() {
            // Find nodes with all dependencies satisfied
            let ready: Vec<String> = remaining
                .iter()
                .filter(|id| {
                    self.nodes
                        .get(*id)
                        .map(|n| n.depends_on.iter().all(|d| completed.contains(d)))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();

            if ready.is_empty() && !remaining.is_empty() {
                return Err(anyhow::anyhow!("Circular dependency detected"));
            }

            // Sort by priority within level
            let mut level: Vec<String> = ready.clone();
            level.sort_by(|a, b| {
                let pa = self.nodes.get(a).map(|n| n.priority).unwrap_or(0);
                let pb = self.nodes.get(b).map(|n| n.priority).unwrap_or(0);
                pb.cmp(&pa) // Higher priority first
            });

            for id in &level {
                remaining.remove(id);
                completed.insert(id.clone());
            }

            self.execution_order.push(level);
        }

        Ok(())
    }

    /// Get execution levels
    pub fn levels(&self) -> &[Vec<String>] {
        &self.execution_order
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Option<&DependencyNode> {
        self.nodes.get(id)
    }

    /// Update node status
    pub fn set_status(&mut self, id: &str, status: NodeStatus) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = status;
        }
    }

    /// Get all nodes
    pub fn nodes(&self) -> &HashMap<String, DependencyNode> {
        &self.nodes
    }

    /// Check if graph has any nodes
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get node count
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Analyze tool calls and build a dependency graph
pub struct DependencyAnalyzer {
    /// Known read-only tools
    read_only_tools: HashSet<String>,
    /// Known write tools
    write_tools: HashSet<String>,
}

impl DependencyAnalyzer {
    /// Create a new analyzer
    pub fn new() -> Self {
        let read_only_tools: HashSet<String> = [
            "file_read",
            "directory_tree",
            "grep_search",
            "glob_find",
            "symbol_search",
            "git_status",
            "git_diff",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let write_tools: HashSet<String> = [
            "file_write",
            "file_edit",
            "git_commit",
            "git_push",
            "shell_exec",
            "cargo_test",
            "cargo_check",
            "cargo_clippy",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            read_only_tools,
            write_tools,
        }
    }

    /// Build a dependency graph from tool calls
    pub fn analyze(&self, calls: &[(String, ParsedToolCall)]) -> DependencyGraph {
        let mut graph = DependencyGraph::new();

        // Add all nodes
        for (id, call) in calls {
            graph.add_node(id, &call.tool_name, call.arguments.clone());
        }

        // Detect dependencies
        for i in 0..calls.len() {
            for j in (i + 1)..calls.len() {
                let (id_i, call_i) = &calls[i];
                let (id_j, call_j) = &calls[j];

                if self.has_dependency(call_i, call_j) {
                    // j depends on i (must run after i)
                    graph.add_dependency(id_i, id_j);
                }
            }
        }

        // Set priorities based on dependencies
        for (id, _call) in calls {
            if let Some(node) = graph.nodes.get(id) {
                // Priority = number of dependents (more dependents = higher priority)
                let priority = node.dependents.len() as u32;
                graph.set_priority(id, priority);
            }
        }

        // Compute execution levels
        let _ = graph.compute_levels();

        graph
    }

    /// Check if call2 depends on call1
    fn has_dependency(&self, call1: &ParsedToolCall, call2: &ParsedToolCall) -> bool {
        // Write before read on same path
        if self.write_tools.contains(&call1.tool_name) {
            let path1 = extract_path(&call1.arguments);
            let path2 = extract_path(&call2.arguments);

            if let (Some(p1), Some(p2)) = (path1, path2) {
                if p1 == p2 || p2.starts_with(&p1) || p1.starts_with(&p2) {
                    return true;
                }
            }
        }

        // Shell commands might affect anything
        if call1.tool_name == "shell_exec" && self.write_tools.contains(&call2.tool_name) {
            return true;
        }

        // Git operations are sequential
        if call1.tool_name.starts_with("git_")
            && call2.tool_name.starts_with("git_")
            && (!self.read_only_tools.contains(&call1.tool_name)
                || !self.read_only_tools.contains(&call2.tool_name))
        {
            return true;
        }

        false
    }

    /// Check if a tool is read-only
    pub fn is_read_only(&self, tool_name: &str) -> bool {
        self.read_only_tools.contains(tool_name)
    }

    /// Check if a tool is a write operation
    pub fn is_write(&self, tool_name: &str) -> bool {
        self.write_tools.contains(tool_name)
    }
}

impl Default for DependencyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Resource Pooling
// ============================================================================

/// A pooled resource
pub trait PooledResource: Send + Sync {
    /// Reset the resource for reuse
    fn reset(&mut self);

    /// Check if resource is healthy
    fn is_healthy(&self) -> bool;
}

/// Generic resource pool
pub struct ResourcePool<T: PooledResource> {
    available: TokioMutex<VecDeque<T>>,
    max_size: usize,
    in_use: AtomicU64,
    created: AtomicU64,
}

impl<T: PooledResource> ResourcePool<T> {
    /// Create a new resource pool
    pub fn new(max_size: usize) -> Self {
        Self {
            available: TokioMutex::new(VecDeque::with_capacity(max_size)),
            max_size,
            in_use: AtomicU64::new(0),
            created: AtomicU64::new(0),
        }
    }

    /// Acquire a resource from the pool
    pub async fn acquire(&self) -> Option<T> {
        let mut available = self.available.lock().await;
        while let Some(resource) = available.pop_front() {
            if resource.is_healthy() {
                self.in_use.fetch_add(1, Ordering::Relaxed);
                return Some(resource);
            }
        }
        None
    }

    /// Return a resource to the pool
    pub async fn release(&self, mut resource: T) {
        resource.reset();
        self.in_use.fetch_sub(1, Ordering::Relaxed);

        let mut available = self.available.lock().await;
        if available.len() < self.max_size {
            available.push_back(resource);
        }
    }

    /// Add a new resource to the pool
    pub async fn add(&self, resource: T) {
        self.created.fetch_add(1, Ordering::Relaxed);
        let mut available = self.available.lock().await;
        if available.len() < self.max_size {
            available.push_back(resource);
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            max_size: self.max_size,
            available: self.available.try_lock().map(|a| a.len()).unwrap_or(0),
            in_use: self.in_use.load(Ordering::Relaxed),
            total_created: self.created.load(Ordering::Relaxed),
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub max_size: usize,
    pub available: usize,
    pub in_use: u64,
    pub total_created: u64,
}

/// HTTP connection pool resource
pub struct HttpConnection {
    created_at: u64,
    healthy: bool,
}

impl HttpConnection {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            created_at: now,
            healthy: true,
        }
    }
}

impl Default for HttpConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl PooledResource for HttpConnection {
    fn reset(&mut self) {
        // Reset connection state
    }

    fn is_healthy(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Connections older than 5 minutes are unhealthy
        self.healthy && (now - self.created_at) < 300
    }
}

/// File handle pool resource
pub struct FileHandle {
    path: String,
    read_only: bool,
    healthy: bool,
}

impl FileHandle {
    pub fn new(path: &str, read_only: bool) -> Self {
        Self {
            path: path.to_string(),
            read_only,
            healthy: true,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
}

impl PooledResource for FileHandle {
    fn reset(&mut self) {
        // Reset file handle state
    }

    fn is_healthy(&self) -> bool {
        self.healthy
    }
}

// ============================================================================
// Batch Coordination
// ============================================================================

/// Configuration for batch execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Maximum wait time before executing batch
    pub max_wait_ms: u64,
    /// Minimum batch size to trigger early execution
    pub min_batch_trigger: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            max_wait_ms: 100,
            min_batch_trigger: 5,
        }
    }
}

/// A batch of similar tool calls
#[derive(Debug)]
pub struct ToolBatch {
    /// Tool name for this batch
    pub tool_name: String,
    /// Calls in this batch
    pub calls: Vec<(String, serde_json::Value)>, // (id, args)
    /// Created timestamp
    pub created_at: u64,
}

impl ToolBatch {
    pub fn new(tool_name: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            tool_name: tool_name.to_string(),
            calls: Vec::new(),
            created_at: now,
        }
    }

    pub fn add(&mut self, id: &str, args: serde_json::Value) {
        self.calls.push((id.to_string(), args));
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    pub fn is_ready(&self, config: &BatchConfig) -> bool {
        if self.calls.len() >= config.max_batch_size {
            return true;
        }
        if self.calls.len() >= config.min_batch_trigger {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            return (now - self.created_at * 1000) >= config.max_wait_ms;
        }
        false
    }
}

/// Coordinates batching of similar tool calls
pub struct BatchCoordinator {
    config: BatchConfig,
    batches: TokioRwLock<HashMap<String, ToolBatch>>,
    stats: BatchStats,
}

/// Batch execution statistics
#[derive(Debug, Default)]
pub struct BatchStats {
    pub total_batches: AtomicU64,
    pub total_calls: AtomicU64,
    pub avg_batch_size: AtomicU64, // Stored as size * 100 for precision
}

impl BatchCoordinator {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            batches: TokioRwLock::new(HashMap::new()),
            stats: BatchStats::default(),
        }
    }

    /// Add a call to be batched
    pub async fn add(&self, tool_name: &str, call_id: &str, args: serde_json::Value) {
        let mut batches = self.batches.write().await;
        let batch = batches
            .entry(tool_name.to_string())
            .or_insert_with(|| ToolBatch::new(tool_name));
        batch.add(call_id, args);
    }

    /// Get ready batches
    pub async fn get_ready_batches(&self) -> Vec<ToolBatch> {
        let mut batches = self.batches.write().await;
        let mut ready = Vec::new();

        let ready_keys: Vec<String> = batches
            .iter()
            .filter(|(_, b)| b.is_ready(&self.config))
            .map(|(k, _)| k.clone())
            .collect();

        for key in ready_keys {
            if let Some(batch) = batches.remove(&key) {
                self.stats.total_batches.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .total_calls
                    .fetch_add(batch.len() as u64, Ordering::Relaxed);
                ready.push(batch);
            }
        }

        ready
    }

    /// Flush all pending batches
    pub async fn flush(&self) -> Vec<ToolBatch> {
        let mut batches = self.batches.write().await;
        let result: Vec<ToolBatch> = batches.drain().map(|(_, b)| b).collect();

        for batch in &result {
            self.stats.total_batches.fetch_add(1, Ordering::Relaxed);
            self.stats
                .total_calls
                .fetch_add(batch.len() as u64, Ordering::Relaxed);
        }

        result
    }

    /// Get statistics
    pub fn stats(&self) -> BatchStatsSummary {
        let total_batches = self.stats.total_batches.load(Ordering::Relaxed);
        let total_calls = self.stats.total_calls.load(Ordering::Relaxed);
        let avg_size = if total_batches > 0 {
            total_calls as f64 / total_batches as f64
        } else {
            0.0
        };

        BatchStatsSummary {
            total_batches,
            total_calls,
            average_batch_size: avg_size,
        }
    }
}

impl Default for BatchCoordinator {
    fn default() -> Self {
        Self::new(BatchConfig::default())
    }
}

/// Summary of batch statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatsSummary {
    pub total_batches: u64,
    pub total_calls: u64,
    pub average_batch_size: f64,
}

// ============================================================================
// Execution Statistics
// ============================================================================

/// Tracks parallel execution performance
pub struct ExecutionStats {
    /// Total executions
    total: AtomicU64,
    /// Parallel executions
    parallel: AtomicU64,
    /// Sequential executions
    sequential: AtomicU64,
    /// Total time saved (ms)
    time_saved_ms: AtomicU64,
    /// Execution history
    history: StdRwLock<VecDeque<ExecutionRecord>>,
}

/// Record of an execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub timestamp: u64,
    pub tool_count: usize,
    pub parallel_count: usize,
    pub sequential_count: usize,
    pub total_duration_ms: u64,
    pub estimated_sequential_ms: u64,
    pub time_saved_ms: u64,
}

impl ExecutionStats {
    pub fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            parallel: AtomicU64::new(0),
            sequential: AtomicU64::new(0),
            time_saved_ms: AtomicU64::new(0),
            history: StdRwLock::new(VecDeque::with_capacity(100)),
        }
    }

    /// Record an execution
    pub fn record(&self, record: ExecutionRecord) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.parallel
            .fetch_add(record.parallel_count as u64, Ordering::Relaxed);
        self.sequential
            .fetch_add(record.sequential_count as u64, Ordering::Relaxed);
        self.time_saved_ms
            .fetch_add(record.time_saved_ms, Ordering::Relaxed);

        if let Ok(mut history) = self.history.write() {
            history.push_back(record);
            while history.len() > 100 {
                history.pop_front();
            }
        }
    }

    /// Get total executions
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Get parallel execution count
    pub fn parallel(&self) -> u64 {
        self.parallel.load(Ordering::Relaxed)
    }

    /// Get sequential execution count
    pub fn sequential(&self) -> u64 {
        self.sequential.load(Ordering::Relaxed)
    }

    /// Get total time saved
    pub fn time_saved_ms(&self) -> u64 {
        self.time_saved_ms.load(Ordering::Relaxed)
    }

    /// Get parallelization ratio
    pub fn parallelization_ratio(&self) -> f64 {
        let total = self.parallel() + self.sequential();
        if total > 0 {
            self.parallel() as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get history
    pub fn history(&self) -> Vec<ExecutionRecord> {
        self.history
            .read()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get summary
    pub fn summary(&self) -> ExecutionStatsSummary {
        ExecutionStatsSummary {
            total_executions: self.total(),
            parallel_calls: self.parallel(),
            sequential_calls: self.sequential(),
            time_saved_ms: self.time_saved_ms(),
            parallelization_ratio: self.parallelization_ratio(),
        }
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.total.store(0, Ordering::Relaxed);
        self.parallel.store(0, Ordering::Relaxed);
        self.sequential.store(0, Ordering::Relaxed);
        self.time_saved_ms.store(0, Ordering::Relaxed);
        if let Ok(mut history) = self.history.write() {
            history.clear();
        }
    }
}

impl Default for ExecutionStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStatsSummary {
    pub total_executions: u64,
    pub parallel_calls: u64,
    pub sequential_calls: u64,
    pub time_saved_ms: u64,
    pub parallelization_ratio: f64,
}

// ============================================================================
// Original Functions
// ============================================================================

/// Estimate if tools are independent (can run in parallel)
pub fn are_independent(tool1: &ParsedToolCall, tool2: &ParsedToolCall) -> bool {
    // Read-only tools are always independent
    let read_only_tools = [
        "file_read",
        "directory_tree",
        "grep_search",
        "glob_find",
        "symbol_search",
        "git_status",
        "git_diff",
    ];

    let tool1_read_only = read_only_tools.contains(&tool1.tool_name.as_str());
    let tool2_read_only = read_only_tools.contains(&tool2.tool_name.as_str());

    // Both read-only = independent
    if tool1_read_only && tool2_read_only {
        return true;
    }

    // Check for path conflicts
    let path1 = extract_path(&tool1.arguments);
    let path2 = extract_path(&tool2.arguments);

    match (path1, path2) {
        (Some(p1), Some(p2)) => {
            // Different paths = potentially independent
            // Same path with at least one write = not independent
            if p1 == p2 && (!tool1_read_only || !tool2_read_only) {
                return false;
            }
            // Check if one is a parent of the other
            !p1.starts_with(&p2) && !p2.starts_with(&p1)
        }
        _ => {
            // No paths = conservative - assume dependent
            tool1_read_only && tool2_read_only
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_parser::ParseMethod;

    fn make_call(name: &str, args: serde_json::Value) -> ParsedToolCall {
        let raw_text = format!("{}:{}", name, args);
        ParsedToolCall {
            tool_name: name.to_string(),
            arguments: args,
            raw_text,
            parse_method: ParseMethod::Xml,
        }
    }

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_concurrency, 4);
        assert!(config.sequential_only.contains("file_write"));
        assert!(config.sequential_only.contains("shell_exec"));
    }

    #[test]
    fn test_can_parallelize_read_only() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        assert!(executor.can_parallelize("file_read"));
        assert!(executor.can_parallelize("directory_tree"));
        assert!(executor.can_parallelize("grep_search"));
        assert!(executor.can_parallelize("git_status"));
    }

    #[test]
    fn test_cannot_parallelize_write_tools() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        assert!(!executor.can_parallelize("file_write"));
        assert!(!executor.can_parallelize("file_edit"));
        assert!(!executor.can_parallelize("shell_exec"));
        assert!(!executor.can_parallelize("git_commit"));
    }

    #[test]
    fn test_analyze_calls_groups_correctly() {
        let executor = ParallelExecutor::new(ParallelConfig::default());

        let calls = [
            make_call("file_read", serde_json::json!({"path": "a.txt"})),
            make_call("grep_search", serde_json::json!({"pattern": "test"})),
            make_call("file_write", serde_json::json!({"path": "b.txt"})),
            make_call("directory_tree", serde_json::json!({"path": "."})),
        ];

        let (parallel, sequential) = executor.analyze_calls(&calls.iter().collect::<Vec<_>>());

        // file_read, grep_search, directory_tree can be parallel
        assert_eq!(parallel.len(), 3);
        // file_write must be sequential
        assert_eq!(sequential.len(), 1);
        assert_eq!(sequential[0].tool_name, "file_write");
    }

    #[test]
    fn test_are_independent_read_only() {
        let call1 = make_call("file_read", serde_json::json!({"path": "a.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "b.txt"}));

        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_are_independent_same_path_read_only() {
        let call1 = make_call("file_read", serde_json::json!({"path": "a.txt"}));
        let call2 = make_call("grep_search", serde_json::json!({"path": "a.txt"}));

        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_same_path_write() {
        let call1 = make_call("file_read", serde_json::json!({"path": "a.txt"}));
        let call2 = make_call("file_write", serde_json::json!({"path": "a.txt"}));

        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_independent_read_only_parent_path() {
        // Two read-only operations are always independent, even on parent/child paths
        let call1 = make_call("file_read", serde_json::json!({"path": "/home/user"}));
        let call2 = make_call(
            "file_read",
            serde_json::json!({"path": "/home/user/file.txt"}),
        );

        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_write_parent_path() {
        // Write to parent + read from child = not independent
        let call1 = make_call(
            "file_write",
            serde_json::json!({"path": "/home/user/file.txt"}),
        );
        let call2 = make_call(
            "file_read",
            serde_json::json!({"path": "/home/user/file.txt"}),
        );

        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_path_conflict_resolution() {
        let executor = ParallelExecutor::new(ParallelConfig::default());

        let call1 = make_call("file_read", serde_json::json!({"path": "a.txt"}));
        let call2 = make_call("grep_search", serde_json::json!({"path": "a.txt"}));
        let call3 = make_call("file_read", serde_json::json!({"path": "b.txt"}));

        let calls = vec![&call1, &call2, &call3];
        let resolved = executor.resolve_path_conflicts(calls);

        // Should keep first occurrence of each path
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn test_disabled_parallel() {
        let config = ParallelConfig {
            enabled: false,
            ..Default::default()
        };
        let executor = ParallelExecutor::new(config);

        // Even read-only tools shouldn't parallelize when disabled
        assert!(!executor.can_parallelize("file_read"));
    }

    #[test]
    fn test_extract_path() {
        let args = serde_json::json!({"path": "/home/test.txt"});
        assert_eq!(extract_path(&args), Some("/home/test.txt".to_string()));

        let args = serde_json::json!({"file": "/home/file.txt"});
        assert_eq!(extract_path(&args), Some("/home/file.txt".to_string()));

        let args = serde_json::json!({"other": "value"});
        assert_eq!(extract_path(&args), None);
    }

    #[tokio::test]
    async fn test_execute_sequential_single() {
        // This test verifies the sequential execution path compiles
        // Full integration test would require a real registry
        let executor = ParallelExecutor::new(ParallelConfig::default());
        assert_eq!(executor.config.max_concurrency, 4);
    }

    #[test]
    fn test_parallel_result_debug() {
        let result = ParallelResult {
            tool_name: "file_read".to_string(),
            tool_call_id: "call_1".to_string(),
            result: Ok(serde_json::json!({"content": "test"})),
            duration_ms: 42,
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("file_read"));
        assert!(debug_str.contains("42"));
    }
}

#[cfg(test)]
mod dependency_graph_tests {
    use super::*;

    #[test]
    fn test_dependency_graph_new() {
        let graph = DependencyGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_dependency_graph_add_node() {
        let mut graph = DependencyGraph::new();
        graph.add_node(
            "node1",
            "file_read",
            serde_json::json!({"path": "test.txt"}),
        );

        assert_eq!(graph.len(), 1);
        assert!(graph.get_node("node1").is_some());
    }

    #[test]
    fn test_dependency_graph_add_dependency() {
        let mut graph = DependencyGraph::new();
        graph.add_node("n1", "file_read", serde_json::json!({}));
        graph.add_node("n2", "file_write", serde_json::json!({}));
        graph.add_dependency("n1", "n2");

        let n2 = graph.get_node("n2").unwrap();
        assert!(n2.depends_on.contains(&"n1".to_string()));

        let n1 = graph.get_node("n1").unwrap();
        assert!(n1.dependents.contains(&"n2".to_string()));
    }

    #[test]
    fn test_dependency_graph_compute_levels() {
        let mut graph = DependencyGraph::new();
        graph.add_node("n1", "file_read", serde_json::json!({}));
        graph.add_node("n2", "file_read", serde_json::json!({}));
        graph.add_node("n3", "file_write", serde_json::json!({}));
        graph.add_dependency("n1", "n3");
        graph.add_dependency("n2", "n3");

        graph.compute_levels().unwrap();

        let levels = graph.levels();
        assert_eq!(levels.len(), 2);
        // n1 and n2 can run in parallel (level 0)
        assert!(levels[0].contains(&"n1".to_string()) || levels[0].contains(&"n2".to_string()));
        // n3 must wait (level 1)
        assert!(levels[1].contains(&"n3".to_string()));
    }

    #[test]
    fn test_dependency_graph_set_priority() {
        let mut graph = DependencyGraph::new();
        graph.add_node("n1", "file_read", serde_json::json!({}));
        graph.set_priority("n1", 10);

        let node = graph.get_node("n1").unwrap();
        assert_eq!(node.priority, 10);
    }

    #[test]
    fn test_dependency_graph_set_status() {
        let mut graph = DependencyGraph::new();
        graph.add_node("n1", "file_read", serde_json::json!({}));
        graph.set_status("n1", NodeStatus::Running);

        let node = graph.get_node("n1").unwrap();
        assert_eq!(node.status, NodeStatus::Running);
    }
}

#[cfg(test)]
mod dependency_analyzer_tests {
    use super::*;
    use crate::tool_parser::ParseMethod;

    fn make_call(name: &str, args: serde_json::Value) -> ParsedToolCall {
        let raw_text = format!("{}:{}", name, args);
        ParsedToolCall {
            tool_name: name.to_string(),
            arguments: args,
            raw_text,
            parse_method: ParseMethod::Xml,
        }
    }

    #[test]
    fn test_analyzer_is_read_only() {
        let analyzer = DependencyAnalyzer::new();
        assert!(analyzer.is_read_only("file_read"));
        assert!(analyzer.is_read_only("git_status"));
        assert!(!analyzer.is_read_only("file_write"));
    }

    #[test]
    fn test_analyzer_is_write() {
        let analyzer = DependencyAnalyzer::new();
        assert!(analyzer.is_write("file_write"));
        assert!(analyzer.is_write("file_edit"));
        assert!(!analyzer.is_write("file_read"));
    }

    #[test]
    fn test_analyzer_analyze_independent() {
        let analyzer = DependencyAnalyzer::new();
        let calls = vec![
            (
                "c1".to_string(),
                make_call("file_read", serde_json::json!({"path": "a.txt"})),
            ),
            (
                "c2".to_string(),
                make_call("file_read", serde_json::json!({"path": "b.txt"})),
            ),
        ];

        let mut graph = analyzer.analyze(&calls);
        assert_eq!(graph.len(), 2);

        let _ = graph.compute_levels();
        // Both can run in parallel
        assert_eq!(graph.levels().len(), 1);
    }

    #[test]
    fn test_analyzer_analyze_dependent() {
        let analyzer = DependencyAnalyzer::new();
        let calls = vec![
            (
                "c1".to_string(),
                make_call("file_write", serde_json::json!({"path": "a.txt"})),
            ),
            (
                "c2".to_string(),
                make_call("file_read", serde_json::json!({"path": "a.txt"})),
            ),
        ];

        let mut graph = analyzer.analyze(&calls);
        let _ = graph.compute_levels();

        // c2 depends on c1
        let n2 = graph.get_node("c2").unwrap();
        assert!(n2.depends_on.contains(&"c1".to_string()));
    }
}

#[cfg(test)]
mod resource_pool_tests {
    use super::*;

    #[tokio::test]
    async fn test_http_connection_pool() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(5);
        pool.add(HttpConnection::new()).await;

        let stats = pool.stats();
        assert_eq!(stats.max_size, 5);
        assert_eq!(stats.available, 1);
        assert_eq!(stats.total_created, 1);
    }

    #[tokio::test]
    async fn test_pool_acquire_release() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(5);
        pool.add(HttpConnection::new()).await;

        let conn = pool.acquire().await;
        assert!(conn.is_some());
        assert_eq!(pool.stats().in_use, 1);

        pool.release(conn.unwrap()).await;
        assert_eq!(pool.stats().in_use, 0);
    }

    #[tokio::test]
    async fn test_pool_empty_acquire() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(5);
        let conn = pool.acquire().await;
        assert!(conn.is_none());
    }

    #[test]
    fn test_http_connection_health() {
        let conn = HttpConnection::new();
        assert!(conn.is_healthy());
    }

    #[test]
    fn test_file_handle() {
        let handle = FileHandle::new("/tmp/test.txt", true);
        assert_eq!(handle.path(), "/tmp/test.txt");
        assert!(handle.is_read_only());
        assert!(handle.is_healthy());
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    #[test]
    fn test_batch_config_default() {
        let config = BatchConfig::default();
        assert_eq!(config.max_batch_size, 10);
        assert_eq!(config.max_wait_ms, 100);
        assert_eq!(config.min_batch_trigger, 5);
    }

    #[test]
    fn test_tool_batch_new() {
        let batch = ToolBatch::new("file_read");
        assert_eq!(batch.tool_name, "file_read");
        assert!(batch.is_empty());
    }

    #[test]
    fn test_tool_batch_add() {
        let mut batch = ToolBatch::new("file_read");
        batch.add("c1", serde_json::json!({"path": "a.txt"}));
        batch.add("c2", serde_json::json!({"path": "b.txt"}));

        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
    }

    #[test]
    fn test_tool_batch_ready_max_size() {
        let config = BatchConfig {
            max_batch_size: 2,
            ..Default::default()
        };

        let mut batch = ToolBatch::new("file_read");
        batch.add("c1", serde_json::json!({}));
        assert!(!batch.is_ready(&config));

        batch.add("c2", serde_json::json!({}));
        assert!(batch.is_ready(&config));
    }

    #[tokio::test]
    async fn test_batch_coordinator_add() {
        let coordinator = BatchCoordinator::default();
        coordinator
            .add("file_read", "c1", serde_json::json!({}))
            .await;
        coordinator
            .add("file_read", "c2", serde_json::json!({}))
            .await;

        let batches = coordinator.flush().await;
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
    }

    #[tokio::test]
    async fn test_batch_coordinator_stats() {
        let coordinator = BatchCoordinator::default();
        coordinator
            .add("file_read", "c1", serde_json::json!({}))
            .await;
        coordinator.flush().await;

        let stats = coordinator.stats();
        assert_eq!(stats.total_batches, 1);
        assert_eq!(stats.total_calls, 1);
    }
}

#[cfg(test)]
mod execution_stats_tests {
    use super::*;

    #[test]
    fn test_execution_stats_new() {
        let stats = ExecutionStats::new();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.parallel(), 0);
        assert_eq!(stats.sequential(), 0);
    }

    #[test]
    fn test_execution_stats_record() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 5,
            parallel_count: 3,
            sequential_count: 2,
            total_duration_ms: 100,
            estimated_sequential_ms: 200,
            time_saved_ms: 100,
        });

        assert_eq!(stats.total(), 1);
        assert_eq!(stats.parallel(), 3);
        assert_eq!(stats.sequential(), 2);
        assert_eq!(stats.time_saved_ms(), 100);
    }

    #[test]
    fn test_execution_stats_parallelization_ratio() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 10,
            parallel_count: 8,
            sequential_count: 2,
            total_duration_ms: 50,
            estimated_sequential_ms: 100,
            time_saved_ms: 50,
        });

        let ratio = stats.parallelization_ratio();
        assert!((ratio - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_execution_stats_summary() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 4,
            parallel_count: 3,
            sequential_count: 1,
            total_duration_ms: 100,
            estimated_sequential_ms: 150,
            time_saved_ms: 50,
        });

        let summary = stats.summary();
        assert_eq!(summary.total_executions, 1);
        assert_eq!(summary.parallel_calls, 3);
        assert_eq!(summary.sequential_calls, 1);
    }

    #[test]
    fn test_execution_stats_reset() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 5,
            parallel_count: 4,
            sequential_count: 1,
            total_duration_ms: 100,
            estimated_sequential_ms: 200,
            time_saved_ms: 100,
        });

        stats.reset();

        assert_eq!(stats.total(), 0);
        assert_eq!(stats.parallel(), 0);
    }
}

// ============================================================================
// Additional comprehensive tests for uncovered lines
// ============================================================================

#[cfg(test)]
mod extract_path_tests {
    use super::*;

    #[test]
    fn test_extract_path_with_directory_key() {
        let args = serde_json::json!({"directory": "/home/user/project"});
        assert_eq!(extract_path(&args), Some("/home/user/project".to_string()));
    }

    #[test]
    fn test_extract_path_prefers_path_over_file() {
        let args = serde_json::json!({"path": "/a.txt", "file": "/b.txt"});
        assert_eq!(extract_path(&args), Some("/a.txt".to_string()));
    }

    #[test]
    fn test_extract_path_falls_back_to_file() {
        let args = serde_json::json!({"file": "/b.txt"});
        assert_eq!(extract_path(&args), Some("/b.txt".to_string()));
    }

    #[test]
    fn test_extract_path_falls_back_to_directory() {
        let args = serde_json::json!({"directory": "/c"});
        assert_eq!(extract_path(&args), Some("/c".to_string()));
    }

    #[test]
    fn test_extract_path_non_string_value() {
        let args = serde_json::json!({"path": 42});
        assert_eq!(extract_path(&args), None);
    }

    #[test]
    fn test_extract_path_empty_object() {
        let args = serde_json::json!({});
        assert_eq!(extract_path(&args), None);
    }

    #[test]
    fn test_extract_path_null_value() {
        let args = serde_json::json!({"path": null});
        assert_eq!(extract_path(&args), None);
    }
}

#[cfg(test)]
mod node_status_tests {
    use super::*;

    #[test]
    fn test_node_status_variants_equality() {
        assert_eq!(NodeStatus::Pending, NodeStatus::Pending);
        assert_eq!(NodeStatus::Ready, NodeStatus::Ready);
        assert_eq!(NodeStatus::Running, NodeStatus::Running);
        assert_eq!(NodeStatus::Completed, NodeStatus::Completed);
        assert_eq!(NodeStatus::Failed, NodeStatus::Failed);
    }

    #[test]
    fn test_node_status_inequality() {
        assert_ne!(NodeStatus::Pending, NodeStatus::Ready);
        assert_ne!(NodeStatus::Running, NodeStatus::Completed);
        assert_ne!(NodeStatus::Completed, NodeStatus::Failed);
    }

    #[test]
    fn test_node_status_clone() {
        let status = NodeStatus::Running;
        let cloned = status;
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_node_status_debug() {
        let debug_str = format!("{:?}", NodeStatus::Pending);
        assert_eq!(debug_str, "Pending");
        assert_eq!(format!("{:?}", NodeStatus::Ready), "Ready");
        assert_eq!(format!("{:?}", NodeStatus::Running), "Running");
        assert_eq!(format!("{:?}", NodeStatus::Completed), "Completed");
        assert_eq!(format!("{:?}", NodeStatus::Failed), "Failed");
    }
}

#[cfg(test)]
mod dependency_graph_extended_tests {
    use super::*;

    #[test]
    fn test_dependency_graph_default() {
        let graph = DependencyGraph::default();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
        assert!(graph.levels().is_empty());
    }

    #[test]
    fn test_add_dependency_duplicate_is_idempotent() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "file_read", serde_json::json!({}));
        graph.add_node("b", "file_write", serde_json::json!({}));

        graph.add_dependency("a", "b");
        graph.add_dependency("a", "b"); // duplicate

        let node_b = graph.get_node("b").unwrap();
        // Should only contain "a" once
        assert_eq!(node_b.depends_on.iter().filter(|d| *d == "a").count(), 1);

        let node_a = graph.get_node("a").unwrap();
        // Should only contain "b" once
        assert_eq!(node_a.dependents.iter().filter(|d| *d == "b").count(), 1);
    }

    #[test]
    fn test_add_dependency_nonexistent_from_node() {
        let mut graph = DependencyGraph::new();
        graph.add_node("b", "file_write", serde_json::json!({}));
        // "a" does not exist
        graph.add_dependency("a", "b");

        let node_b = graph.get_node("b").unwrap();
        assert!(node_b.depends_on.contains(&"a".to_string()));
    }

    #[test]
    fn test_add_dependency_nonexistent_to_node() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "file_read", serde_json::json!({}));
        // "b" does not exist
        graph.add_dependency("a", "b");

        let node_a = graph.get_node("a").unwrap();
        // "b" should NOT be in dependents because "b" node doesn't exist
        // Actually the code still modifies "a" since it does `get_mut(from)`
        assert!(node_a.dependents.contains(&"b".to_string()));
    }

    #[test]
    fn test_compute_levels_circular_dependency() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "tool1", serde_json::json!({}));
        graph.add_node("b", "tool2", serde_json::json!({}));
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "a");

        let result = graph.compute_levels();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Circular dependency"));
    }

    #[test]
    fn test_compute_levels_three_level_chain() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "tool1", serde_json::json!({}));
        graph.add_node("b", "tool2", serde_json::json!({}));
        graph.add_node("c", "tool3", serde_json::json!({}));
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "c");

        graph.compute_levels().unwrap();
        let levels = graph.levels();
        assert_eq!(levels.len(), 3);
        assert!(levels[0].contains(&"a".to_string()));
        assert!(levels[1].contains(&"b".to_string()));
        assert!(levels[2].contains(&"c".to_string()));
    }

    #[test]
    fn test_compute_levels_priority_sorting() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "tool1", serde_json::json!({}));
        graph.add_node("b", "tool2", serde_json::json!({}));
        graph.add_node("c", "tool3", serde_json::json!({}));
        // No dependencies -- all at level 0
        graph.set_priority("a", 1);
        graph.set_priority("b", 10);
        graph.set_priority("c", 5);

        graph.compute_levels().unwrap();
        let levels = graph.levels();
        assert_eq!(levels.len(), 1);
        // Level should be sorted by priority descending: b(10), c(5), a(1)
        assert_eq!(levels[0][0], "b");
        assert_eq!(levels[0][1], "c");
        assert_eq!(levels[0][2], "a");
    }

    #[test]
    fn test_compute_levels_empty_graph() {
        let mut graph = DependencyGraph::new();
        graph.compute_levels().unwrap();
        assert!(graph.levels().is_empty());
    }

    #[test]
    fn test_set_priority_nonexistent_node() {
        let mut graph = DependencyGraph::new();
        // Should not panic
        graph.set_priority("nonexistent", 42);
    }

    #[test]
    fn test_set_status_nonexistent_node() {
        let mut graph = DependencyGraph::new();
        // Should not panic
        graph.set_status("nonexistent", NodeStatus::Failed);
    }

    #[test]
    fn test_set_status_transitions() {
        let mut graph = DependencyGraph::new();
        graph.add_node("n1", "tool", serde_json::json!({}));

        assert_eq!(graph.get_node("n1").unwrap().status, NodeStatus::Pending);

        graph.set_status("n1", NodeStatus::Ready);
        assert_eq!(graph.get_node("n1").unwrap().status, NodeStatus::Ready);

        graph.set_status("n1", NodeStatus::Running);
        assert_eq!(graph.get_node("n1").unwrap().status, NodeStatus::Running);

        graph.set_status("n1", NodeStatus::Completed);
        assert_eq!(graph.get_node("n1").unwrap().status, NodeStatus::Completed);

        graph.set_status("n1", NodeStatus::Failed);
        assert_eq!(graph.get_node("n1").unwrap().status, NodeStatus::Failed);
    }

    #[test]
    fn test_get_node_nonexistent() {
        let graph = DependencyGraph::new();
        assert!(graph.get_node("nonexistent").is_none());
    }

    #[test]
    fn test_nodes_returns_all() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "t1", serde_json::json!({}));
        graph.add_node("b", "t2", serde_json::json!({}));

        let nodes = graph.nodes();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.contains_key("a"));
        assert!(nodes.contains_key("b"));
    }

    #[test]
    fn test_dependency_node_debug_clone() {
        let node = DependencyNode {
            id: "test".to_string(),
            tool_name: "file_read".to_string(),
            arguments: serde_json::json!({"path": "a.txt"}),
            depends_on: vec!["dep1".to_string()],
            dependents: vec!["dep2".to_string()],
            priority: 5,
            status: NodeStatus::Pending,
        };

        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("file_read"));

        let cloned = node.clone();
        assert_eq!(cloned.id, "test");
        assert_eq!(cloned.priority, 5);
        assert_eq!(cloned.status, NodeStatus::Pending);
    }

    #[test]
    fn test_compute_levels_diamond_dependency() {
        // A -> B, A -> C, B -> D, C -> D
        let mut graph = DependencyGraph::new();
        graph.add_node("a", "t1", serde_json::json!({}));
        graph.add_node("b", "t2", serde_json::json!({}));
        graph.add_node("c", "t3", serde_json::json!({}));
        graph.add_node("d", "t4", serde_json::json!({}));
        graph.add_dependency("a", "b");
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "d");
        graph.add_dependency("c", "d");

        graph.compute_levels().unwrap();
        let levels = graph.levels();
        assert_eq!(levels.len(), 3);
        assert!(levels[0].contains(&"a".to_string()));
        assert!(levels[1].contains(&"b".to_string()));
        assert!(levels[1].contains(&"c".to_string()));
        assert!(levels[2].contains(&"d".to_string()));
    }
}

#[cfg(test)]
mod dependency_analyzer_extended_tests {
    use super::*;
    use crate::tool_parser::ParseMethod;

    fn make_call(name: &str, args: serde_json::Value) -> ParsedToolCall {
        let raw_text = format!("{}:{}", name, args);
        ParsedToolCall {
            tool_name: name.to_string(),
            arguments: args,
            raw_text,
            parse_method: ParseMethod::Xml,
        }
    }

    #[test]
    fn test_analyzer_default() {
        let analyzer = DependencyAnalyzer::default();
        assert!(analyzer.is_read_only("file_read"));
        assert!(analyzer.is_write("file_write"));
    }

    #[test]
    fn test_analyzer_read_only_tools_comprehensive() {
        let analyzer = DependencyAnalyzer::new();
        assert!(analyzer.is_read_only("file_read"));
        assert!(analyzer.is_read_only("directory_tree"));
        assert!(analyzer.is_read_only("grep_search"));
        assert!(analyzer.is_read_only("glob_find"));
        assert!(analyzer.is_read_only("symbol_search"));
        assert!(analyzer.is_read_only("git_status"));
        assert!(analyzer.is_read_only("git_diff"));
        assert!(!analyzer.is_read_only("unknown_tool"));
    }

    #[test]
    fn test_analyzer_write_tools_comprehensive() {
        let analyzer = DependencyAnalyzer::new();
        assert!(analyzer.is_write("file_write"));
        assert!(analyzer.is_write("file_edit"));
        assert!(analyzer.is_write("git_commit"));
        assert!(analyzer.is_write("git_push"));
        assert!(analyzer.is_write("shell_exec"));
        assert!(analyzer.is_write("cargo_test"));
        assert!(analyzer.is_write("cargo_check"));
        assert!(analyzer.is_write("cargo_clippy"));
        assert!(!analyzer.is_write("unknown_tool"));
    }

    #[test]
    fn test_has_dependency_write_before_read_same_path() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        assert!(analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_write_parent_path() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("file_write", serde_json::json!({"path": "/home"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/home/file.txt"}));
        assert!(analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_write_child_path() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("file_edit", serde_json::json!({"path": "/home/file.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/home"}));
        assert!(analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_write_different_paths() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/b.txt"}));
        assert!(!analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_shell_exec_before_write() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("shell_exec", serde_json::json!({"command": "rm -rf"}));
        let call2 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        assert!(analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_shell_exec_before_read() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("shell_exec", serde_json::json!({"command": "ls"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        // shell_exec before a non-write tool should not be a dependency
        assert!(!analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_git_write_operations() {
        let analyzer = DependencyAnalyzer::new();
        // git_commit (write) before git_push (write)
        let call1 = make_call("git_commit", serde_json::json!({"message": "test"}));
        let call2 = make_call("git_push", serde_json::json!({}));
        assert!(analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_git_read_then_write() {
        let analyzer = DependencyAnalyzer::new();
        // git_status (read) before git_commit (write) -- at least one is not read-only
        let call1 = make_call("git_status", serde_json::json!({}));
        let call2 = make_call("git_commit", serde_json::json!({"message": "test"}));
        assert!(analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_git_read_only_both() {
        let analyzer = DependencyAnalyzer::new();
        // git_status (read) and git_diff (read) -- both read-only
        let call1 = make_call("git_status", serde_json::json!({}));
        let call2 = make_call("git_diff", serde_json::json!({}));
        assert!(!analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_read_read_no_dependency() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        assert!(!analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_has_dependency_write_no_paths() {
        let analyzer = DependencyAnalyzer::new();
        let call1 = make_call("file_write", serde_json::json!({"content": "hello"}));
        let call2 = make_call("file_read", serde_json::json!({"content": "world"}));
        // write tool but no extractable paths -- no path-based dependency
        assert!(!analyzer.has_dependency(&call1, &call2));
    }

    #[test]
    fn test_analyze_sets_priorities() {
        let analyzer = DependencyAnalyzer::new();
        let calls = vec![
            (
                "c1".to_string(),
                make_call("file_write", serde_json::json!({"path": "/a.txt"})),
            ),
            (
                "c2".to_string(),
                make_call("file_read", serde_json::json!({"path": "/a.txt"})),
            ),
            (
                "c3".to_string(),
                make_call("file_read", serde_json::json!({"path": "/a.txt"})),
            ),
        ];

        let graph = analyzer.analyze(&calls);
        // c1 (write) should have dependents c2 and c3
        let node_c1 = graph.get_node("c1").unwrap();
        // Priority should reflect the number of dependents
        assert!(node_c1.priority > 0);
    }

    #[test]
    fn test_analyze_computes_levels() {
        let analyzer = DependencyAnalyzer::new();
        let calls = vec![
            (
                "c1".to_string(),
                make_call("file_write", serde_json::json!({"path": "/a.txt"})),
            ),
            (
                "c2".to_string(),
                make_call("file_read", serde_json::json!({"path": "/a.txt"})),
            ),
        ];

        let graph = analyzer.analyze(&calls);
        let levels = graph.levels();
        assert_eq!(levels.len(), 2);
    }

    #[test]
    fn test_analyze_all_independent() {
        let analyzer = DependencyAnalyzer::new();
        let calls = vec![
            (
                "c1".to_string(),
                make_call("file_read", serde_json::json!({"path": "/a.txt"})),
            ),
            (
                "c2".to_string(),
                make_call("grep_search", serde_json::json!({"pattern": "foo"})),
            ),
            (
                "c3".to_string(),
                make_call("directory_tree", serde_json::json!({"path": "/other"})),
            ),
        ];

        let graph = analyzer.analyze(&calls);
        let levels = graph.levels();
        // All independent -- single level
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 3);
    }

    #[test]
    fn test_analyze_empty_calls() {
        let analyzer = DependencyAnalyzer::new();
        let calls: Vec<(String, ParsedToolCall)> = vec![];

        let graph = analyzer.analyze(&calls);
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }
}

#[cfg(test)]
mod resource_pool_extended_tests {
    use super::*;

    #[test]
    fn test_http_connection_default() {
        let conn = HttpConnection::default();
        assert!(conn.is_healthy());
    }

    #[test]
    fn test_http_connection_created_at() {
        let conn = HttpConnection::new();
        // created_at should be recent (within last few seconds)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        assert!(conn.created_at <= now);
        assert!(now - conn.created_at < 5);
    }

    #[test]
    fn test_http_connection_unhealthy_flag() {
        let mut conn = HttpConnection::new();
        assert!(conn.is_healthy());
        conn.healthy = false;
        assert!(!conn.is_healthy());
    }

    #[test]
    fn test_http_connection_old_is_unhealthy() {
        let mut conn = HttpConnection::new();
        // Simulate an old connection (older than 5 minutes)
        conn.created_at = 0; // epoch time
        assert!(!conn.is_healthy());
    }

    #[test]
    fn test_file_handle_new_read_write() {
        let handle = FileHandle::new("/tmp/rw.txt", false);
        assert_eq!(handle.path(), "/tmp/rw.txt");
        assert!(!handle.is_read_only());
        assert!(handle.is_healthy());
    }

    #[test]
    fn test_file_handle_reset() {
        let mut handle = FileHandle::new("/tmp/test.txt", true);
        handle.reset(); // Should not panic
        assert!(handle.is_healthy());
    }

    #[test]
    fn test_file_handle_unhealthy() {
        let mut handle = FileHandle::new("/tmp/test.txt", true);
        handle.healthy = false;
        assert!(!handle.is_healthy());
    }

    #[test]
    fn test_pool_stats_initial() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(10);
        let stats = pool.stats();
        assert_eq!(stats.max_size, 10);
        assert_eq!(stats.available, 0);
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.total_created, 0);
    }

    #[test]
    fn test_pool_stats_debug() {
        let stats = PoolStats {
            max_size: 5,
            available: 2,
            in_use: 1,
            total_created: 3,
        };
        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("max_size"));
        assert!(debug_str.contains("5"));
        assert!(debug_str.contains("available"));
        assert!(debug_str.contains("2"));
    }

    #[test]
    fn test_pool_stats_clone() {
        let stats = PoolStats {
            max_size: 5,
            available: 2,
            in_use: 1,
            total_created: 3,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.max_size, 5);
        assert_eq!(cloned.available, 2);
        assert_eq!(cloned.in_use, 1);
        assert_eq!(cloned.total_created, 3);
    }

    #[test]
    fn test_pool_stats_serialize_deserialize() {
        let stats = PoolStats {
            max_size: 5,
            available: 2,
            in_use: 1,
            total_created: 3,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: PoolStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_size, 5);
        assert_eq!(deserialized.available, 2);
    }

    #[tokio::test]
    async fn test_pool_add_beyond_max_size() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(1);
        pool.add(HttpConnection::new()).await;
        pool.add(HttpConnection::new()).await; // Should be silently dropped

        let stats = pool.stats();
        assert_eq!(stats.total_created, 2);
        assert_eq!(stats.available, 1); // Only 1 fits
    }

    #[tokio::test]
    async fn test_pool_release_beyond_max_size() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(1);
        pool.add(HttpConnection::new()).await;

        // Acquire the one available
        let conn1 = pool.acquire().await.unwrap();
        // Now add a second manually
        pool.add(HttpConnection::new()).await;

        // Release conn1 -- pool already has 1 available, so this should be dropped
        pool.release(conn1).await;
        let stats = pool.stats();
        assert_eq!(stats.available, 1);
    }

    #[tokio::test]
    async fn test_pool_acquire_skips_unhealthy() {
        let pool: ResourcePool<HttpConnection> = ResourcePool::new(5);
        // Add an unhealthy connection
        let mut old_conn = HttpConnection::new();
        old_conn.created_at = 0; // Very old -- unhealthy
        pool.add(old_conn).await;

        // Acquire should return None because the only connection is unhealthy
        let result = pool.acquire().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_pool_acquire_finds_healthy_after_unhealthy() {
        let pool: ResourcePool<FileHandle> = ResourcePool::new(5);
        // Add an unhealthy handle
        let mut bad_handle = FileHandle::new("/bad", true);
        bad_handle.healthy = false;
        pool.add(bad_handle).await;
        // Add a healthy handle
        pool.add(FileHandle::new("/good", true)).await;

        let result = pool.acquire().await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().path(), "/good");
    }

    #[tokio::test]
    async fn test_pool_full_lifecycle() {
        let pool: ResourcePool<FileHandle> = ResourcePool::new(3);
        // Add resources
        pool.add(FileHandle::new("/a", true)).await;
        pool.add(FileHandle::new("/b", false)).await;

        assert_eq!(pool.stats().available, 2);
        assert_eq!(pool.stats().total_created, 2);

        // Acquire one
        let handle = pool.acquire().await.unwrap();
        assert_eq!(pool.stats().available, 1);
        assert_eq!(pool.stats().in_use, 1);

        // Release it
        pool.release(handle).await;
        assert_eq!(pool.stats().available, 2);
        assert_eq!(pool.stats().in_use, 0);
    }
}

#[cfg(test)]
mod batch_extended_tests {
    use super::*;

    #[test]
    fn test_batch_config_debug() {
        let config = BatchConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("max_batch_size"));
        assert!(debug_str.contains("10"));
    }

    #[test]
    fn test_batch_config_clone() {
        let config = BatchConfig {
            max_batch_size: 20,
            max_wait_ms: 500,
            min_batch_trigger: 8,
        };
        let cloned = config.clone();
        assert_eq!(cloned.max_batch_size, 20);
        assert_eq!(cloned.max_wait_ms, 500);
        assert_eq!(cloned.min_batch_trigger, 8);
    }

    #[test]
    fn test_batch_config_serialize_deserialize() {
        let config = BatchConfig {
            max_batch_size: 15,
            max_wait_ms: 200,
            min_batch_trigger: 3,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BatchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_batch_size, 15);
        assert_eq!(deserialized.max_wait_ms, 200);
        assert_eq!(deserialized.min_batch_trigger, 3);
    }

    #[test]
    fn test_tool_batch_debug() {
        let batch = ToolBatch::new("test_tool");
        let debug_str = format!("{:?}", batch);
        assert!(debug_str.contains("test_tool"));
    }

    #[test]
    fn test_tool_batch_is_ready_below_min_trigger() {
        let config = BatchConfig {
            max_batch_size: 10,
            max_wait_ms: 100,
            min_batch_trigger: 5,
        };
        let mut batch = ToolBatch::new("tool");
        batch.add("c1", serde_json::json!({}));
        batch.add("c2", serde_json::json!({}));
        // Only 2 items, below min_batch_trigger of 5
        assert!(!batch.is_ready(&config));
    }

    #[test]
    fn test_tool_batch_is_ready_at_max_batch_size() {
        let config = BatchConfig {
            max_batch_size: 3,
            max_wait_ms: 100000,
            min_batch_trigger: 10,
        };
        let mut batch = ToolBatch::new("tool");
        batch.add("c1", serde_json::json!({}));
        batch.add("c2", serde_json::json!({}));
        batch.add("c3", serde_json::json!({}));
        // Reached max_batch_size of 3
        assert!(batch.is_ready(&config));
    }

    #[test]
    fn test_tool_batch_is_ready_min_trigger_recent() {
        // Batch at min_batch_trigger but created very recently
        let config = BatchConfig {
            max_batch_size: 100,
            max_wait_ms: 999999999, // Very long wait
            min_batch_trigger: 2,
        };
        let mut batch = ToolBatch::new("tool");
        batch.add("c1", serde_json::json!({}));
        batch.add("c2", serde_json::json!({}));
        // At min_batch_trigger but hasn't waited long enough
        // created_at is in seconds, max_wait_ms is very large
        assert!(!batch.is_ready(&config));
    }

    #[test]
    fn test_tool_batch_is_ready_min_trigger_old() {
        // Batch at min_batch_trigger and very old
        let config = BatchConfig {
            max_batch_size: 100,
            max_wait_ms: 0, // Zero wait time -- always ready if at trigger
            min_batch_trigger: 2,
        };
        let mut batch = ToolBatch::new("tool");
        batch.add("c1", serde_json::json!({}));
        batch.add("c2", serde_json::json!({}));
        // The time check: (now_ms - created_at * 1000) >= max_wait_ms (0)
        // Since now >= created_at, this should be true
        assert!(batch.is_ready(&config));
    }

    #[tokio::test]
    async fn test_batch_coordinator_multiple_tools() {
        let coordinator = BatchCoordinator::default();
        coordinator
            .add("file_read", "c1", serde_json::json!({"path": "a"}))
            .await;
        coordinator
            .add("grep_search", "c2", serde_json::json!({"pattern": "x"}))
            .await;
        coordinator
            .add("file_read", "c3", serde_json::json!({"path": "b"}))
            .await;

        let batches = coordinator.flush().await;
        assert_eq!(batches.len(), 2); // Two different tool names
    }

    #[tokio::test]
    async fn test_batch_coordinator_get_ready_batches_none_ready() {
        let config = BatchConfig {
            max_batch_size: 100,
            max_wait_ms: 999999999,
            min_batch_trigger: 100,
        };
        let coordinator = BatchCoordinator::new(config);
        coordinator
            .add("file_read", "c1", serde_json::json!({}))
            .await;

        let ready = coordinator.get_ready_batches().await;
        assert!(ready.is_empty());
    }

    #[tokio::test]
    async fn test_batch_coordinator_get_ready_batches_max_size() {
        let config = BatchConfig {
            max_batch_size: 2,
            max_wait_ms: 999999999,
            min_batch_trigger: 100,
        };
        let coordinator = BatchCoordinator::new(config);
        coordinator
            .add("file_read", "c1", serde_json::json!({}))
            .await;
        coordinator
            .add("file_read", "c2", serde_json::json!({}))
            .await;

        let ready = coordinator.get_ready_batches().await;
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].len(), 2);
    }

    #[tokio::test]
    async fn test_batch_coordinator_stats_empty() {
        let coordinator = BatchCoordinator::default();
        let stats = coordinator.stats();
        assert_eq!(stats.total_batches, 0);
        assert_eq!(stats.total_calls, 0);
        assert_eq!(stats.average_batch_size, 0.0);
    }

    #[tokio::test]
    async fn test_batch_coordinator_stats_after_flush() {
        let coordinator = BatchCoordinator::default();
        coordinator.add("tool_a", "c1", serde_json::json!({})).await;
        coordinator.add("tool_a", "c2", serde_json::json!({})).await;
        coordinator.add("tool_b", "c3", serde_json::json!({})).await;

        coordinator.flush().await;

        let stats = coordinator.stats();
        assert_eq!(stats.total_batches, 2);
        assert_eq!(stats.total_calls, 3);
        assert!((stats.average_batch_size - 1.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_batch_coordinator_get_ready_updates_stats() {
        let config = BatchConfig {
            max_batch_size: 1, // Each item is immediately ready
            max_wait_ms: 0,
            min_batch_trigger: 1,
        };
        let coordinator = BatchCoordinator::new(config);
        coordinator
            .add("file_read", "c1", serde_json::json!({}))
            .await;

        let ready = coordinator.get_ready_batches().await;
        assert_eq!(ready.len(), 1);

        let stats = coordinator.stats();
        assert_eq!(stats.total_batches, 1);
        assert_eq!(stats.total_calls, 1);
    }

    #[test]
    fn test_batch_stats_summary_debug() {
        let summary = BatchStatsSummary {
            total_batches: 5,
            total_calls: 20,
            average_batch_size: 4.0,
        };
        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("total_batches"));
        assert!(debug_str.contains("5"));
    }

    #[test]
    fn test_batch_stats_summary_clone() {
        let summary = BatchStatsSummary {
            total_batches: 5,
            total_calls: 20,
            average_batch_size: 4.0,
        };
        let cloned = summary.clone();
        assert_eq!(cloned.total_batches, 5);
        assert_eq!(cloned.total_calls, 20);
        assert_eq!(cloned.average_batch_size, 4.0);
    }

    #[test]
    fn test_batch_stats_summary_serialize_deserialize() {
        let summary = BatchStatsSummary {
            total_batches: 3,
            total_calls: 9,
            average_batch_size: 3.0,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: BatchStatsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_batches, 3);
        assert_eq!(deserialized.total_calls, 9);
    }
}

#[cfg(test)]
mod execution_stats_extended_tests {
    use super::*;

    #[test]
    fn test_execution_stats_default() {
        let stats = ExecutionStats::default();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.parallel(), 0);
        assert_eq!(stats.sequential(), 0);
        assert_eq!(stats.time_saved_ms(), 0);
    }

    #[test]
    fn test_execution_stats_parallelization_ratio_zero() {
        let stats = ExecutionStats::new();
        assert_eq!(stats.parallelization_ratio(), 0.0);
    }

    #[test]
    fn test_execution_stats_parallelization_ratio_all_parallel() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 5,
            parallel_count: 5,
            sequential_count: 0,
            total_duration_ms: 50,
            estimated_sequential_ms: 100,
            time_saved_ms: 50,
        });
        assert_eq!(stats.parallelization_ratio(), 1.0);
    }

    #[test]
    fn test_execution_stats_parallelization_ratio_all_sequential() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 5,
            parallel_count: 0,
            sequential_count: 5,
            total_duration_ms: 100,
            estimated_sequential_ms: 100,
            time_saved_ms: 0,
        });
        assert_eq!(stats.parallelization_ratio(), 0.0);
    }

    #[test]
    fn test_execution_stats_history() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 100,
            tool_count: 2,
            parallel_count: 1,
            sequential_count: 1,
            total_duration_ms: 50,
            estimated_sequential_ms: 80,
            time_saved_ms: 30,
        });
        stats.record(ExecutionRecord {
            timestamp: 200,
            tool_count: 3,
            parallel_count: 2,
            sequential_count: 1,
            total_duration_ms: 60,
            estimated_sequential_ms: 100,
            time_saved_ms: 40,
        });

        let history = stats.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].timestamp, 100);
        assert_eq!(history[1].timestamp, 200);
    }

    #[test]
    fn test_execution_stats_history_overflow() {
        let stats = ExecutionStats::new();
        // Record 105 entries -- should cap at 100
        for i in 0..105 {
            stats.record(ExecutionRecord {
                timestamp: i as u64,
                tool_count: 1,
                parallel_count: 1,
                sequential_count: 0,
                total_duration_ms: 10,
                estimated_sequential_ms: 10,
                time_saved_ms: 0,
            });
        }

        let history = stats.history();
        assert_eq!(history.len(), 100);
        // Oldest entries should have been removed (0..4 removed, 5..104 remain)
        assert_eq!(history[0].timestamp, 5);
        assert_eq!(history[99].timestamp, 104);
    }

    #[test]
    fn test_execution_stats_summary_fields() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 10,
            parallel_count: 7,
            sequential_count: 3,
            total_duration_ms: 100,
            estimated_sequential_ms: 200,
            time_saved_ms: 100,
        });

        let summary = stats.summary();
        assert_eq!(summary.total_executions, 1);
        assert_eq!(summary.parallel_calls, 7);
        assert_eq!(summary.sequential_calls, 3);
        assert_eq!(summary.time_saved_ms, 100);
        assert!((summary.parallelization_ratio - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_execution_stats_multiple_records() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 5,
            parallel_count: 3,
            sequential_count: 2,
            total_duration_ms: 50,
            estimated_sequential_ms: 100,
            time_saved_ms: 50,
        });
        stats.record(ExecutionRecord {
            timestamp: 1,
            tool_count: 4,
            parallel_count: 2,
            sequential_count: 2,
            total_duration_ms: 40,
            estimated_sequential_ms: 80,
            time_saved_ms: 40,
        });

        assert_eq!(stats.total(), 2);
        assert_eq!(stats.parallel(), 5); // 3 + 2
        assert_eq!(stats.sequential(), 4); // 2 + 2
        assert_eq!(stats.time_saved_ms(), 90); // 50 + 40
    }

    #[test]
    fn test_execution_stats_reset_clears_history() {
        let stats = ExecutionStats::new();
        stats.record(ExecutionRecord {
            timestamp: 0,
            tool_count: 1,
            parallel_count: 1,
            sequential_count: 0,
            total_duration_ms: 10,
            estimated_sequential_ms: 10,
            time_saved_ms: 0,
        });

        stats.reset();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.parallel(), 0);
        assert_eq!(stats.sequential(), 0);
        assert_eq!(stats.time_saved_ms(), 0);
        assert!(stats.history().is_empty());
    }

    #[test]
    fn test_execution_record_debug() {
        let record = ExecutionRecord {
            timestamp: 12345,
            tool_count: 3,
            parallel_count: 2,
            sequential_count: 1,
            total_duration_ms: 100,
            estimated_sequential_ms: 150,
            time_saved_ms: 50,
        };
        let debug_str = format!("{:?}", record);
        assert!(debug_str.contains("12345"));
        assert!(debug_str.contains("tool_count"));
    }

    #[test]
    fn test_execution_record_clone() {
        let record = ExecutionRecord {
            timestamp: 12345,
            tool_count: 3,
            parallel_count: 2,
            sequential_count: 1,
            total_duration_ms: 100,
            estimated_sequential_ms: 150,
            time_saved_ms: 50,
        };
        let cloned = record.clone();
        assert_eq!(cloned.timestamp, 12345);
        assert_eq!(cloned.tool_count, 3);
        assert_eq!(cloned.time_saved_ms, 50);
    }

    #[test]
    fn test_execution_record_serialize_deserialize() {
        let record = ExecutionRecord {
            timestamp: 1000,
            tool_count: 5,
            parallel_count: 3,
            sequential_count: 2,
            total_duration_ms: 200,
            estimated_sequential_ms: 400,
            time_saved_ms: 200,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ExecutionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timestamp, 1000);
        assert_eq!(deserialized.time_saved_ms, 200);
    }

    #[test]
    fn test_execution_stats_summary_debug() {
        let summary = ExecutionStatsSummary {
            total_executions: 10,
            parallel_calls: 30,
            sequential_calls: 5,
            time_saved_ms: 500,
            parallelization_ratio: 0.857,
        };
        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("total_executions"));
        assert!(debug_str.contains("10"));
    }

    #[test]
    fn test_execution_stats_summary_serialize_deserialize() {
        let summary = ExecutionStatsSummary {
            total_executions: 10,
            parallel_calls: 30,
            sequential_calls: 5,
            time_saved_ms: 500,
            parallelization_ratio: 0.857,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: ExecutionStatsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_executions, 10);
        assert_eq!(deserialized.parallel_calls, 30);
    }
}

#[cfg(test)]
mod are_independent_extended_tests {
    use super::*;
    use crate::tool_parser::ParseMethod;

    fn make_call(name: &str, args: serde_json::Value) -> ParsedToolCall {
        let raw_text = format!("{}:{}", name, args);
        ParsedToolCall {
            tool_name: name.to_string(),
            arguments: args,
            raw_text,
            parse_method: ParseMethod::Xml,
        }
    }

    #[test]
    fn test_independent_different_paths_both_write() {
        let call1 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_write", serde_json::json!({"path": "/b.txt"}));
        // Different paths, even though both are writes
        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_same_path_write_write() {
        let call1 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_parent_child_paths_with_write() {
        let call1 = make_call("file_write", serde_json::json!({"path": "/home"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/home/file.txt"}));
        // Parent-child path with at least one write
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_child_parent_paths_with_write() {
        let call1 = make_call("file_read", serde_json::json!({"path": "/home/file.txt"}));
        let call2 = make_call("file_write", serde_json::json!({"path": "/home"}));
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_no_paths_non_read_only() {
        // tool1 is not read-only, tool2 is not read-only, no paths
        let call1 = make_call("shell_exec", serde_json::json!({"command": "ls"}));
        let call2 = make_call("cargo_test", serde_json::json!({"args": "--all"}));
        // No paths, both non-read-only => conservative: returns tool1_read_only && tool2_read_only = false
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_no_paths_one_read_only() {
        let call1 = make_call("file_read", serde_json::json!({"pattern": "foo"}));
        let call2 = make_call("shell_exec", serde_json::json!({"command": "ls"}));
        // No extractable paths, one read-only and one not => false
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_independent_no_paths_both_read_only() {
        let call1 = make_call("git_status", serde_json::json!({}));
        let call2 = make_call("git_diff", serde_json::json!({}));
        // Both read-only => true (early return)
        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_independent_different_paths_read_write() {
        let call1 = make_call("file_read", serde_json::json!({"path": "/x.txt"}));
        let call2 = make_call("file_write", serde_json::json!({"path": "/y.txt"}));
        // Different paths, even with one write
        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_independent_one_has_path_other_does_not() {
        let call1 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("shell_exec", serde_json::json!({"command": "echo hello"}));
        // call1 has path, call2 does not -> (Some, None) arm
        // This goes to the _ arm: returns tool1_read_only && tool2_read_only
        // tool1 is read_only, tool2 is NOT read_only => false
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_not_independent_same_path_read_write_reversed() {
        let call1 = make_call("file_write", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        assert!(!are_independent(&call1, &call2));
    }

    #[test]
    fn test_independent_both_read_only_same_path() {
        let call1 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        let call2 = make_call("file_read", serde_json::json!({"path": "/a.txt"}));
        // Both read-only -- early return true
        assert!(are_independent(&call1, &call2));
    }

    #[test]
    fn test_independent_directory_key_extraction() {
        let call1 = make_call("directory_tree", serde_json::json!({"directory": "/home"}));
        let call2 = make_call("directory_tree", serde_json::json!({"directory": "/var"}));
        // Both read-only, both have directory paths -- independent
        assert!(are_independent(&call1, &call2));
    }
}

#[cfg(test)]
mod parallel_config_extended_tests {
    use super::*;

    #[test]
    fn test_parallel_config_debug() {
        let config = ParallelConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("max_concurrency"));
        assert!(debug_str.contains("4"));
        assert!(debug_str.contains("enabled"));
        assert!(debug_str.contains("true"));
    }

    #[test]
    fn test_parallel_config_clone() {
        let config = ParallelConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_concurrency, config.max_concurrency);
        assert_eq!(cloned.enabled, config.enabled);
        assert_eq!(cloned.sequential_only.len(), config.sequential_only.len());
    }

    #[test]
    fn test_parallel_config_default_sequential_tools() {
        let config = ParallelConfig::default();
        assert!(config.sequential_only.contains("file_write"));
        assert!(config.sequential_only.contains("file_edit"));
        assert!(config.sequential_only.contains("git_commit"));
        assert!(config.sequential_only.contains("git_push"));
        assert!(config.sequential_only.contains("shell_exec"));
        assert_eq!(config.sequential_only.len(), 5);
    }

    #[test]
    fn test_parallel_config_custom() {
        let mut sequential = HashSet::new();
        sequential.insert("custom_tool".to_string());
        let config = ParallelConfig {
            max_concurrency: 8,
            enabled: false,
            sequential_only: sequential,
        };
        assert_eq!(config.max_concurrency, 8);
        assert!(!config.enabled);
        assert!(config.sequential_only.contains("custom_tool"));
    }
}

#[cfg(test)]
mod parallel_executor_extended_tests {
    use super::*;
    use crate::tool_parser::ParseMethod;

    fn make_call(name: &str, args: serde_json::Value) -> ParsedToolCall {
        let raw_text = format!("{}:{}", name, args);
        ParsedToolCall {
            tool_name: name.to_string(),
            arguments: args,
            raw_text,
            parse_method: ParseMethod::Xml,
        }
    }

    #[test]
    fn test_analyze_calls_all_sequential() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        let calls = [
            make_call("file_write", serde_json::json!({"path": "a.txt"})),
            make_call("file_edit", serde_json::json!({"path": "b.txt"})),
            make_call("shell_exec", serde_json::json!({"command": "ls"})),
        ];

        let (parallel, sequential) = executor.analyze_calls(&calls.iter().collect::<Vec<_>>());
        assert!(parallel.is_empty());
        assert_eq!(sequential.len(), 3);
    }

    #[test]
    fn test_analyze_calls_all_parallel() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        let calls = [
            make_call("file_read", serde_json::json!({"path": "a.txt"})),
            make_call("grep_search", serde_json::json!({"pattern": "test"})),
            make_call("directory_tree", serde_json::json!({"path": "/other"})),
        ];

        let (parallel, sequential) = executor.analyze_calls(&calls.iter().collect::<Vec<_>>());
        assert_eq!(parallel.len(), 3);
        assert!(sequential.is_empty());
    }

    #[test]
    fn test_analyze_calls_empty() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        let calls: Vec<ParsedToolCall> = vec![];
        let (parallel, sequential) = executor.analyze_calls(&calls.iter().collect::<Vec<_>>());
        assert!(parallel.is_empty());
        assert!(sequential.is_empty());
    }

    #[test]
    fn test_analyze_calls_path_conflict_moves_to_sequential() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        let calls = [
            make_call("file_read", serde_json::json!({"path": "same.txt"})),
            make_call("grep_search", serde_json::json!({"path": "same.txt"})),
        ];

        let (parallel, sequential) = executor.analyze_calls(&calls.iter().collect::<Vec<_>>());
        // First keeps same.txt, second conflicts and goes to sequential
        assert_eq!(parallel.len(), 1);
        assert_eq!(sequential.len(), 1);
    }

    #[test]
    fn test_resolve_path_conflicts_no_paths() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        let call1 = make_call("grep_search", serde_json::json!({"pattern": "foo"}));
        let call2 = make_call("grep_search", serde_json::json!({"pattern": "bar"}));

        let calls = vec![&call1, &call2];
        let resolved = executor.resolve_path_conflicts(calls);
        // No paths to conflict on
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn test_resolve_path_conflicts_mixed_paths_and_no_paths() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        let call1 = make_call("file_read", serde_json::json!({"path": "a.txt"}));
        let call2 = make_call("grep_search", serde_json::json!({"pattern": "foo"}));
        let call3 = make_call("file_read", serde_json::json!({"path": "a.txt"}));

        let calls = vec![&call1, &call2, &call3];
        let resolved = executor.resolve_path_conflicts(calls);
        // call1 kept (path a.txt), call2 kept (no path), call3 dropped (path a.txt conflict)
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn test_can_parallelize_custom_tool() {
        let executor = ParallelExecutor::new(ParallelConfig::default());
        // Custom tool not in sequential_only
        assert!(executor.can_parallelize("my_custom_tool"));
    }

    #[test]
    fn test_parallel_result_with_error() {
        let result = ParallelResult {
            tool_name: "failing_tool".to_string(),
            tool_call_id: "call_err".to_string(),
            result: Err(anyhow::anyhow!("Something went wrong")),
            duration_ms: 0,
        };
        assert!(result.result.is_err());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("failing_tool"));
    }
}

#[cfg(test)]
mod batch_stats_tests {
    use super::*;

    #[test]
    fn test_batch_stats_default() {
        let stats = BatchStats::default();
        assert_eq!(stats.total_batches.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_calls.load(Ordering::Relaxed), 0);
        assert_eq!(stats.avg_batch_size.load(Ordering::Relaxed), 0);
    }
}
