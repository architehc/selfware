//! Parallel Execution Types

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};

use crate::tool_parser::ParsedToolCall;

/// Configuration for parallel execution
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Maximum number of concurrent tool executions
    pub max_concurrency: usize,
    /// Whether parallel execution is enabled
    pub enabled: bool,
    /// Tools that should never run in parallel
    pub sequential_only: HashSet<String>,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        let mut sequential_only = HashSet::new();
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
    pub result: anyhow::Result<serde_json::Value>,
    pub duration_ms: u64,
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

/// A node in the dependency graph
#[derive(Debug, Clone)]
pub struct DependencyNode {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub depends_on: Vec<String>,
    pub dependents: Vec<String>,
    pub priority: u32,
    pub status: NodeStatus,
}

/// Dependency graph for tool execution ordering
pub struct DependencyGraph {
    pub(crate) nodes: HashMap<String, DependencyNode>,
    pub(crate) execution_order: Vec<Vec<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            execution_order: Vec::new(),
        }
    }

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

    pub fn set_priority(&mut self, id: &str, priority: u32) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.priority = priority;
        }
    }

    pub fn compute_levels(&mut self) -> anyhow::Result<()> {
        self.execution_order.clear();
        let mut remaining: HashSet<String> = self.nodes.keys().cloned().collect();
        let mut completed: HashSet<String> = HashSet::new();

        while !remaining.is_empty() {
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

            let mut level: Vec<String> = ready.clone();
            level.sort_by(|a, b| {
                let pa = self.nodes.get(a).map(|n| n.priority).unwrap_or(0);
                let pb = self.nodes.get(b).map(|n| n.priority).unwrap_or(0);
                pb.cmp(&pa)
            });

            for id in &level {
                remaining.remove(id);
                completed.insert(id.clone());
            }

            self.execution_order.push(level);
        }

        Ok(())
    }

    pub fn levels(&self) -> &[Vec<String>] {
        &self.execution_order
    }

    pub fn get_node(&self, id: &str) -> Option<&DependencyNode> {
        self.nodes.get(id)
    }

    pub fn set_status(&mut self, id: &str, status: NodeStatus) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = status;
        }
    }

    pub fn nodes(&self) -> &HashMap<String, DependencyNode> {
        &self.nodes
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub max_wait_ms: u64,
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

/// Batch of similar tool calls
#[derive(Debug)]
pub struct ToolBatch {
    pub tool_name: String,
    pub calls: Vec<(String, serde_json::Value)>,
    pub created_at: u64,
}

impl ToolBatch {
    pub fn new(tool_name: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            return (now - self.created_at * 1000) >= config.max_wait_ms;
        }
        false
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

/// Execution statistics
pub struct ExecutionStats {
    total: AtomicU64,
    parallel: AtomicU64,
    sequential: AtomicU64,
    time_saved_ms: AtomicU64,
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

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn parallel(&self) -> u64 {
        self.parallel.load(Ordering::Relaxed)
    }

    pub fn sequential(&self) -> u64 {
        self.sequential.load(Ordering::Relaxed)
    }

    pub fn time_saved_ms(&self) -> u64 {
        self.time_saved_ms.load(Ordering::Relaxed)
    }

    pub fn parallelization_ratio(&self) -> f64 {
        let total = self.parallel() + self.sequential();
        if total > 0 {
            self.parallel() as f64 / total as f64
        } else {
            0.0
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
