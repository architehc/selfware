//! Parallel Execution Engine

use super::types::{
    DependencyGraph, ExecutionRecord, ExecutionStats, ParallelConfig, ParallelResult,
};
use crate::tools::ToolRegistry;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use tokio::sync::{RwLock, Semaphore};

/// Parallel executor for tool calls
pub struct ParallelExecutor {
    config: Arc<RwLock<ParallelConfig>>,
    semaphore: Arc<Semaphore>,
    active_count: Arc<AtomicU64>,
    queue: Arc<StdRwLock<VecDeque<QueuedToolCall>>>,
    stats: Arc<ExecutionStats>,
    paused: Arc<AtomicBool>,
}

struct QueuedToolCall {
    tool_name: String,
    arguments: serde_json::Value,
    call_id: String,
    depends_on: Vec<String>,
    priority: u32,
}

impl ParallelExecutor {
    pub fn new(config: ParallelConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
        Self {
            config: Arc::new(RwLock::new(config)),
            semaphore,
            active_count: Arc::new(AtomicU64::new(0)),
            queue: Arc::new(StdRwLock::new(VecDeque::new())),
            stats: Arc::new(ExecutionStats::new()),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_concurrency(max_concurrency: usize) -> Self {
        let mut config = ParallelConfig::default();
        config.max_concurrency = max_concurrency;
        Self::new(config)
    }

    pub async fn enqueue(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        call_id: Option<String>,
    ) -> anyhow::Result<()> {
        if self.paused.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Executor is paused"));
        }

        let config = self.config.read().await;
        if config.sequential_only.contains(tool_name) {
            drop(config);
            self.enqueue_sequential(tool_name, arguments, call_id).await
        } else {
            let call_id = call_id.unwrap_or_else(|| self.generate_id());
            let call = QueuedToolCall {
                tool_name: tool_name.to_string(),
                arguments,
                call_id,
                depends_on: Vec::new(),
                priority: 0,
            };
            drop(config);

            let mut queue = self
                .queue
                .write()
                .map_err(|_| anyhow::anyhow!("Queue lock poisoned"))?;
            queue.push_back(call);
            Ok(())
        }
    }

    async fn enqueue_sequential(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        call_id: Option<String>,
    ) -> anyhow::Result<()> {
        let call_id = call_id.unwrap_or_else(|| self.generate_id());
        let call = QueuedToolCall {
            tool_name: tool_name.to_string(),
            arguments,
            call_id,
            depends_on: Vec::new(),
            priority: 100,
        };

        let mut queue = self
            .queue
            .write()
            .map_err(|_| anyhow::anyhow!("Queue lock poisoned"))?;
        queue.push_back(call);
        Ok(())
    }

    fn generate_id(&self) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        format!("call_{}", COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub async fn execute_batch(
        &self,
        calls: Vec<(String, serde_json::Value)>,
        registry: Arc<ToolRegistry>,
    ) -> Vec<ParallelResult> {
        if calls.is_empty() {
            return Vec::new();
        }

        let start_time = std::time::Instant::now();
        let config = self.config.read().await;
        if !config.enabled || calls.len() == 1 {
            drop(config);
            let mut results = Vec::with_capacity(calls.len());
            for (tool_name, arguments) in calls {
                let call_id = self.generate_id();
                let result = registry.execute(&tool_name, arguments).await;
                results.push(ParallelResult {
                    tool_name,
                    tool_call_id: call_id,
                    result,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
            }
            return results;
        }
        drop(config);

        let semaphore = self.semaphore.clone();
        let mut handles = Vec::with_capacity(calls.len());

        for (tool_name, arguments) in calls {
            let call_id = self.generate_id();
            let registry = registry.clone();
            let permit = semaphore.clone().acquire_owned().await;
            let active = self.active_count.clone();

            handles.push(tokio::spawn(async move {
                let _permit = permit;
                active.fetch_add(1, Ordering::Relaxed);
                let call_start = std::time::Instant::now();
                let result = registry.execute(&tool_name, arguments).await;
                active.fetch_sub(1, Ordering::Relaxed);
                ParallelResult {
                    tool_name,
                    tool_call_id: call_id,
                    result,
                    duration_ms: call_start.elapsed().as_millis() as u64,
                }
            }));
        }

        let results: Vec<ParallelResult> = futures::future::join_all(handles)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let elapsed = start_time.elapsed().as_millis() as u64;
        let estimated_sequential = results.iter().map(|r| r.duration_ms).sum::<u64>();
        let time_saved = estimated_sequential.saturating_sub(elapsed);

        self.stats.record(ExecutionRecord {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tool_count: results.len(),
            parallel_count: results.len(),
            sequential_count: 0,
            total_duration_ms: elapsed,
            estimated_sequential_ms: estimated_sequential,
            time_saved_ms: time_saved,
        });

        results
    }

    pub async fn execute_with_dependencies(
        &self,
        graph: &mut DependencyGraph,
        registry: Arc<ToolRegistry>,
    ) -> HashMap<String, ParallelResult> {
        let _ = graph.compute_levels();
        let levels = graph.levels().to_vec();
        let mut results: HashMap<String, ParallelResult> = HashMap::new();

        for level in levels {
            if level.is_empty() {
                continue;
            }

            if level.len() == 1 {
                let id = &level[0];
                if let Some(node) = graph.get_node(id) {
                    let start = std::time::Instant::now();
                    let tool_name = node.tool_name.clone();
                    let arguments = node.arguments.clone();
                    let result = registry.execute(&tool_name, arguments).await;
                    results.insert(
                        id.clone(),
                        ParallelResult {
                            tool_name,
                            tool_call_id: id.clone(),
                            result,
                            duration_ms: start.elapsed().as_millis() as u64,
                        },
                    );
                }
            } else {
                let calls: Vec<(String, String, serde_json::Value)> = level
                    .iter()
                    .filter_map(|id| {
                        graph
                            .get_node(id)
                            .map(|n| (id.clone(), n.tool_name.clone(), n.arguments.clone()))
                    })
                    .collect();

                let semaphore = self.semaphore.clone();
                let mut handles = Vec::with_capacity(calls.len());

                for (id, tool_name, arguments) in calls {
                    let registry = registry.clone();
                    let permit = semaphore.clone().acquire_owned().await;
                    let active = self.active_count.clone();

                    handles.push(tokio::spawn(async move {
                        let _permit = permit;
                        active.fetch_add(1, Ordering::Relaxed);
                        let start = std::time::Instant::now();
                        let result = registry.execute(&tool_name, arguments).await;
                        active.fetch_sub(1, Ordering::Relaxed);
                        (id, tool_name, result, start.elapsed().as_millis() as u64)
                    }));
                }

                let level_results = futures::future::join_all(handles).await;
                for result in level_results {
                    if let Ok((id, tool_name, res, duration)) = result {
                        results.insert(
                            id.clone(),
                            ParallelResult {
                                tool_name,
                                tool_call_id: id,
                                result: res,
                                duration_ms: duration,
                            },
                        );
                    }
                }
            }
        }

        results
    }

    pub async fn active_count(&self) -> u64 {
        self.active_count.load(Ordering::Relaxed)
    }

    pub async fn pending_count(&self) -> usize {
        self.queue.read().map(|q| q.len()).unwrap_or(0)
    }

    pub fn stats(&self) -> &ExecutionStats {
        &self.stats
    }

    pub async fn set_max_concurrency(&self, max: usize) {
        let mut config = self.config.write().await;
        config.max_concurrency = max;
    }

    pub async fn enable(&self) {
        let mut config = self.config.write().await;
        config.enabled = true;
    }

    pub async fn disable(&self) {
        let mut config = self.config.write().await;
        config.enabled = false;
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }
}

impl Clone for ParallelExecutor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            semaphore: self.semaphore.clone(),
            active_count: self.active_count.clone(),
            queue: self.queue.clone(),
            stats: self.stats.clone(),
            paused: self.paused.clone(),
        }
    }
}
