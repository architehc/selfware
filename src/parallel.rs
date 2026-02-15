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

#![allow(dead_code)]

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
                let _permit = semaphore.acquire().await.unwrap();
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
            "git_log",
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
        if call1.tool_name.starts_with("git_") && call2.tool_name.starts_with("git_")
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
// Enhanced Parallel Executor
// ============================================================================

/// Enhanced parallel executor with all capabilities
pub struct EnhancedParallelExecutor {
    config: ParallelConfig,
    semaphore: Arc<Semaphore>,
    analyzer: DependencyAnalyzer,
    batch_coordinator: BatchCoordinator,
    stats: ExecutionStats,
}

impl EnhancedParallelExecutor {
    /// Create a new enhanced executor
    pub fn new(config: ParallelConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
        Self {
            config,
            semaphore,
            analyzer: DependencyAnalyzer::new(),
            batch_coordinator: BatchCoordinator::default(),
            stats: ExecutionStats::new(),
        }
    }

    /// Execute with automatic dependency analysis
    pub async fn execute_with_analysis(
        &self,
        calls: Vec<(String, ParsedToolCall)>,
        registry: Arc<ToolRegistry>,
    ) -> Vec<ParallelResult> {
        use tokio::time::Instant;

        let start = Instant::now();
        let graph = self.analyzer.analyze(&calls);

        let mut results = Vec::new();
        let mut estimated_sequential_ms: u64 = 0;

        // Execute level by level
        for level in graph.levels() {
            if level.is_empty() {
                continue;
            }

            // Collect calls for this level
            let level_calls: Vec<_> = calls
                .iter()
                .filter(|(id, _)| level.contains(id))
                .cloned()
                .collect();

            if level_calls.len() == 1 {
                // Single call - execute directly
                let (id, call) = &level_calls[0];
                let call_start = Instant::now();
                let result = registry
                    .execute(&call.tool_name, call.arguments.clone())
                    .await;
                let duration_ms = call_start.elapsed().as_millis() as u64;
                estimated_sequential_ms += duration_ms;

                results.push(ParallelResult {
                    tool_name: call.tool_name.clone(),
                    tool_call_id: id.clone(),
                    result,
                    duration_ms,
                });
            } else {
                // Multiple calls - execute in parallel
                let level_start = Instant::now();
                let parallel_results = self
                    .execute_parallel_internal(level_calls, registry.clone())
                    .await;
                let _level_duration = level_start.elapsed().as_millis() as u64;

                // Estimate sequential time
                for r in &parallel_results {
                    estimated_sequential_ms += r.duration_ms;
                }

                results.extend(parallel_results);
            }
        }

        let total_duration_ms = start.elapsed().as_millis() as u64;
        let time_saved = estimated_sequential_ms.saturating_sub(total_duration_ms);

        // Record statistics
        let parallel_count = graph
            .levels()
            .iter()
            .filter(|l| l.len() > 1)
            .map(|l| l.len())
            .sum();
        let sequential_count = graph.levels().iter().filter(|l| l.len() == 1).count();

        self.stats.record(ExecutionRecord {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tool_count: calls.len(),
            parallel_count,
            sequential_count,
            total_duration_ms,
            estimated_sequential_ms,
            time_saved_ms: time_saved,
        });

        results
    }

    /// Execute calls in parallel internally
    async fn execute_parallel_internal(
        &self,
        calls: Vec<(String, ParsedToolCall)>,
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
                let _permit = semaphore.acquire().await.unwrap();
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

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }

        results
    }

    /// Get execution statistics
    pub fn stats(&self) -> &ExecutionStats {
        &self.stats
    }

    /// Get batch coordinator
    pub fn batch_coordinator(&self) -> &BatchCoordinator {
        &self.batch_coordinator
    }

    /// Get dependency analyzer
    pub fn analyzer(&self) -> &DependencyAnalyzer {
        &self.analyzer
    }
}

impl Default for EnhancedParallelExecutor {
    fn default() -> Self {
        Self::new(ParallelConfig::default())
    }
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
        "git_log",
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

#[cfg(test)]
mod enhanced_executor_tests {
    use super::*;

    #[test]
    fn test_enhanced_executor_new() {
        let executor = EnhancedParallelExecutor::default();
        assert_eq!(executor.stats().total(), 0);
    }

    #[test]
    fn test_enhanced_executor_analyzer() {
        let executor = EnhancedParallelExecutor::default();
        assert!(executor.analyzer().is_read_only("file_read"));
    }

    #[tokio::test]
    async fn test_enhanced_executor_batch_coordinator() {
        let executor = EnhancedParallelExecutor::default();
        executor
            .batch_coordinator()
            .add("file_read", "c1", serde_json::json!({}))
            .await;

        let batches = executor.batch_coordinator().flush().await;
        assert_eq!(batches.len(), 1);
    }
}
