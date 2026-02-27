# Selfware Multi-Day Autonomous Execution Infrastructure Design

## Executive Summary

This document presents a comprehensive infrastructure design for Selfware to support 3-7+ days of autonomous execution with recursive self-improvement using local Qwen3 Coder (1M context). The design addresses checkpointing, process supervision, resource management, LLM inference optimization, observability, and human oversight.

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Selfware Autonomous Runtime                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│  │   Agent      │  │   Agent      │  │   Agent      │  │   Agent      │    │
│  │   Worker 1   │  │   Worker 2   │  │   Worker N   │  │  Supervisor  │    │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘    │
│         │                 │                 │                 │            │
│         └─────────────────┴─────────────────┴─────────────────┘            │
│                                    │                                        │
│  ┌─────────────────────────────────┴─────────────────────────────────┐     │
│  │                    Checkpoint & State Manager                      │     │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │     │
│  │  │  Task      │  │  System    │  │  Model     │  │  Recovery  │  │     │
│  │  │  State     │  │  State     │  │  Cache     │  │  Journal   │  │     │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │     │
│  └──────────────────────────────────────────────────────────────────┘     │
│                                    │                                        │
│  ┌─────────────────────────────────┴─────────────────────────────────┐     │
│  │                    Resource Manager                                │     │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │     │
│  │  │   GPU      │  │  Memory    │  │   Disk     │  │   Circuit  │  │     │
│  │  │  Monitor   │  │  Monitor   │  │  Monitor   │  │  Breaker   │  │     │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │     │
│  └──────────────────────────────────────────────────────────────────┘     │
│                                    │                                        │
│  ┌─────────────────────────────────┴─────────────────────────────────┐     │
│  │                    LLM Inference Manager                           │     │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │     │
│  │  │   vLLM     │  │   Model    │  │  Quantize  │  │   Request  │  │     │
│  │  │  Engine    │  │  Lifecycle │  │   Manager  │  │   Queue    │  │     │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │     │
│  └──────────────────────────────────────────────────────────────────┘     │
│                                    │                                        │
│  ┌─────────────────────────────────┴─────────────────────────────────┐     │
│  │                    Observability Stack                             │     │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │     │
│  │  │   Logs     │  │  Metrics   │  │   Traces   │  │   Health   │  │     │
│  │  │  (tracing) │  │ (prometheus)│  │  (jaeger)  │  │   Check    │  │     │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │     │
│  └──────────────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Checkpointing Strategy

### 2.1 Checkpoint Granularity Levels

| Level | Frequency | Size | Purpose |
|-------|-----------|------|---------|
| **Micro** | Every 30s | 10-100KB | Token stream position, partial outputs |
| **Task** | On completion | 1-10MB | Full task state, results |
| **Session** | Every 5min + graceful | 10-100MB | Complete agent state |
| **System** | Every 15min | 100MB-1GB | Full system snapshot |

### 2.2 Checkpoint Storage Hierarchy

```rust
// checkpoint/storage.rs
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub struct CheckpointStorage {
    /// Hot storage: In-memory for immediate recovery (last 5 checkpoints)
    hot_cache: Arc<RwLock<LruCache<CheckpointId, Checkpoint>>>,
    
    /// Warm storage: Local SSD for fast recovery (last 24h)
    warm_storage: PathBuf,
    
    /// Cold storage: Compressed archives for historical (7+ days)
    cold_storage: Option<PathBuf>,
    
    /// Differential storage: Only store changes
    diff_engine: DiffEngine,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: CheckpointLevel,
    pub state: CheckpointState,
    pub parent: Option<CheckpointId>,
    pub diff_from_parent: Option<Vec<u8>>, // Binary diff
}

#[derive(Serialize, Deserialize, Clone)]
pub enum CheckpointState {
    Micro(MicroCheckpoint),
    Task(TaskCheckpoint),
    Session(SessionCheckpoint),
    System(SystemCheckpoint),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionCheckpoint {
    pub agent_state: AgentState,
    pub conversation_history: Vec<Message>,
    pub pending_tasks: Vec<Task>,
    pub completed_tasks: Vec<CompletedTask>,
    pub context_window: ContextWindowState,
    pub model_cache_state: ModelCacheState,
    pub metrics: SessionMetrics,
}
```

### 2.3 Incremental Checkpointing with Content-Defined Chunking

```rust
// checkpoint/incremental.rs
use fastcdc::FastCDC;
use blake3::Hasher;

pub struct IncrementalCheckpointManager {
    chunker: FastCDC,
    chunk_store: ChunkStore,
}

impl IncrementalCheckpointManager {
    /// Create checkpoint using content-defined chunking
    /// Only stores chunks that have changed
    pub async fn create_incremental(
        &self,
        previous: &Checkpoint,
        current: &SessionState,
    ) -> Result<Checkpoint, CheckpointError> {
        let serialized = bincode::serialize(current)?;
        
        // Content-defined chunking for deduplication
        let chunks: Vec<Chunk> = self.chunker.chunk(&serialized)
            .map(|c| Chunk {
                hash: blake3::hash(c.data),
                data: c.data.to_vec(),
            })
            .collect();
        
        // Store only new chunks
        let new_chunks: Vec<Chunk> = self.chunk_store
            .filter_existing(&chunks)
            .await?;
        
        self.chunk_store.store_batch(&new_chunks).await?;
        
        Ok(Checkpoint {
            chunks: chunks.iter().map(|c| c.hash).collect(),
            metadata: CheckpointMetadata::new(),
        })
    }
}
```

### 2.4 Checkpoint Frequency Strategy

```rust
// checkpoint/scheduler.rs
pub struct CheckpointScheduler {
    config: CheckpointConfig,
    last_checkpoint: Instant,
    pending_changes: AtomicU64,
}

#[derive(Clone)]
pub struct CheckpointConfig {
    /// Minimum time between checkpoints
    pub min_interval: Duration,
    
    /// Maximum time without checkpoint
    pub max_interval: Duration,
    
    /// Checkpoint after N significant changes
    pub changes_threshold: u64,
    
    /// Adaptive: checkpoint more frequently under load
    pub adaptive: bool,
    
    /// Compression level (0-9)
    pub compression: u32,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            min_interval: Duration::from_secs(30),
            max_interval: Duration::from_secs(300),
            changes_threshold: 100,
            adaptive: true,
            compression: 6,
        }
    }
}

impl CheckpointScheduler {
    pub fn should_checkpoint(&self, state: &SystemState) -> bool {
        let elapsed = self.last_checkpoint.elapsed();
        let changes = self.pending_changes.load(Ordering::Relaxed);
        
        // Always checkpoint after max_interval
        if elapsed >= self.config.max_interval {
            return true;
        }
        
        // Don't checkpoint too frequently
        if elapsed < self.config.min_interval {
            return false;
        }
        
        // Checkpoint after significant changes
        if changes >= self.config.changes_threshold {
            return true;
        }
        
        // Adaptive: checkpoint more frequently under memory pressure
        if self.config.adaptive && state.memory_pressure > 0.8 {
            return elapsed >= self.config.min_interval.mul_f32(0.5);
        }
        
        false
    }
}
```

### 2.5 Recovery Protocol

```rust
// checkpoint/recovery.rs
pub struct RecoveryManager {
    checkpoint_store: Arc<CheckpointStorage>,
    recovery_journal: RecoveryJournal,
}

impl RecoveryManager {
    /// Recover from crash - automatic on startup
    pub async fn recover(&self) -> Result<RecoveredState, RecoveryError> {
        // 1. Check for ungraceful shutdown
        let last_session = self.recovery_journal.get_last_session().await?;
        
        match last_session.shutdown_type {
            ShutdownType::Graceful => {
                // Normal startup
                Ok(RecoveredState::Clean(last_session.state))
            }
            ShutdownType::Crash | ShutdownType::Unknown => {
                // Attempt recovery
                self.attempt_crash_recovery(&last_session).await
            }
        }
    }
    
    async fn attempt_crash_recovery(
        &self,
        session: &SessionRecord,
    ) -> Result<RecoveredState, RecoveryError> {
        // 1. Find most recent valid checkpoint
        let checkpoint = self.find_latest_valid_checkpoint().await?;
        
        // 2. Replay recovery journal
        let journal_entries = self.recovery_journal
            .get_entries_since(checkpoint.timestamp)
            .await?;
        
        // 3. Apply journal entries idempotently
        let mut state = checkpoint.state.clone();
        for entry in journal_entries {
            match entry.apply(&mut state).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Journal entry failed: {:?}, continuing", e);
                    // Continue - some entries may be partial
                }
            }
        }
        
        // 4. Verify state consistency
        if self.verify_state_consistency(&state).await? {
            Ok(RecoveredState::Recovered(state))
        } else {
            // Fall back to earlier checkpoint
            self.attempt_earlier_recovery().await
        }
    }
}
```

---

## 3. Process Supervision and Recovery

### 3.1 Hierarchical Supervision Tree

```rust
// supervision/tree.rs
use tokio::sync::mpsc;

/// Actor-style supervision tree inspired by Erlang/OTP
pub struct Supervisor {
    id: SupervisorId,
    strategy: SupervisionStrategy,
    children: Vec<ChildSpec>,
    restart_policy: RestartPolicy,
}

#[derive(Clone, Copy, Debug)]
pub enum SupervisionStrategy {
    /// Restart all children if one fails
    OneForAll,
    /// Restart only the failed child
    OneForOne,
    /// Restart failed child and all children started after it
    RestForOne,
}

#[derive(Clone, Debug)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub max_seconds: u32,
    pub backoff_strategy: BackoffStrategy,
}

#[derive(Clone, Debug)]
pub enum BackoffStrategy {
    Fixed(Duration),
    Linear { base: Duration, multiplier: f32 },
    Exponential { base: Duration, max: Duration },
}

impl Supervisor {
    pub async fn start(self) -> Result<SupervisorHandle, SupervisionError> {
        let (tx, rx) = mpsc::channel(100);
        
        tokio::spawn(async move {
            self.supervision_loop(rx).await;
        });
        
        Ok(SupervisorHandle { tx })
    }
    
    async fn supervision_loop(&self, mut rx: mpsc::Receiver<ChildEvent>) {
        let mut restart_counts: HashMap<ChildId, Vec<Instant>> = HashMap::new();
        
        while let Some(event) = rx.recv().await {
            match event {
                ChildEvent::Crashed { child_id, error } => {
                    tracing::error!("Child {} crashed: {:?}", child_id, error);
                    
                    // Check restart intensity
                    if self.should_restart(&child_id, &mut restart_counts) {
                        self.restart_child(child_id).await;
                    } else {
                        self.escalate(child_id, error).await;
                    }
                }
                ChildEvent::Exited { child_id, reason } => {
                    if reason != ExitReason::Normal {
                        self.handle_abnormal_exit(child_id, reason).await;
                    }
                }
            }
        }
    }
}
```

### 3.2 Process Monitor with Health Checks

```rust
// supervision/health.rs
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct HealthMonitor {
    checks: Vec<Box<dyn HealthCheck + Send + Sync>>,
    interval: Duration,
    failure_threshold: u32,
}

#[async_trait]
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self) -> HealthStatus;
}

#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String, severity: Severity },
}

pub struct AgentHealthCheck {
    last_heartbeat: Arc<RwLock<Option<Instant>>>,
    heartbeat_timeout: Duration,
}

#[async_trait]
impl HealthCheck for AgentHealthCheck {
    fn name(&self) -> &str {
        "agent_heartbeat"
    }
    
    async fn check(&self) -> HealthStatus {
        let last = *self.last_heartbeat.read().await;
        
        match last {
            None => HealthStatus::Unhealthy {
                reason: "No heartbeat received".to_string(),
                severity: Severity::Critical,
            },
            Some(instant) => {
                let elapsed = instant.elapsed();
                if elapsed > self.heartbeat_timeout * 2 {
                    HealthStatus::Unhealthy {
                        reason: format!("Heartbeat timeout: {:?}", elapsed),
                        severity: Severity::Critical,
                    }
                } else if elapsed > self.heartbeat_timeout {
                    HealthStatus::Degraded {
                        reason: format!("Slow heartbeat: {:?}", elapsed),
                    }
                } else {
                    HealthStatus::Healthy
                }
            }
        }
    }
}
```

### 3.3 Circuit Breaker Pattern

```rust
// supervision/circuit_breaker.rs
use std::sync::atomic::{AtomicU32, Ordering};

pub struct CircuitBreaker {
    state: AtomicU32, // 0=Closed, 1=Open, 2=HalfOpen
    failure_count: AtomicU32,
    success_count: AtomicU32,
    config: CircuitBreakerConfig,
    last_failure_time: RwLock<Option<Instant>>,
}

#[derive(Clone)]
pub struct CircuitBreakerConfig {
    /// Failures before opening circuit
    pub failure_threshold: u32,
    /// Successes in half-open before closing
    pub success_threshold: u32,
    /// Time before attempting half-open
    pub reset_timeout: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f32,
}

impl CircuitBreaker {
    pub async fn call<F, Fut, T, E>(
        &self,
        operation: F,
    ) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        match self.current_state().await {
            CircuitState::Open => {
                if self.should_attempt_reset().await {
                    self.transition_to(CircuitState::HalfOpen).await;
                } else {
                    return Err(CircuitBreakerError::CircuitOpen);
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited through
            }
            CircuitState::Closed => {}
        }
        
        match operation().await {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(e) => {
                self.on_failure().await;
                Err(CircuitBreakerError::OperationFailed(e))
            }
        }
    }
    
    async fn on_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure_time.write().await = Some(Instant::now());
        
        if count >= self.config.failure_threshold {
            self.transition_to(CircuitState::Open).await;
        }
    }
}
```

### 3.4 Watchdog Timer

```rust
// supervision/watchdog.rs
pub struct Watchdog {
    timeout: Duration,
    last_pet: Arc<RwLock<Instant>>,
    action: Box<dyn Fn() + Send + Sync>,
}

impl Watchdog {
    pub fn new(timeout: Duration, action: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            timeout,
            last_pet: Arc::new(RwLock::new(Instant::now())),
            action: Box::new(action),
        }
    }
    
    pub fn pet(&self) {
        *self.last_pet.write() = Instant::now();
    }
    
    pub async fn run(&self) {
        loop {
            tokio::time::sleep(self.timeout / 2).await;
            
            let last = *self.last_pet.read().await;
            if last.elapsed() > self.timeout {
                tracing::error!("Watchdog timeout! Triggering recovery action");
                (self.action)();
            }
        }
    }
}

/// Application-level watchdog for the entire system
pub struct SystemWatchdog {
    watchdogs: Vec<ComponentWatchdog>,
    global_timeout: Duration,
}

impl SystemWatchdog {
    pub async fn monitor(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            for watchdog in &self.watchdogs {
                if watchdog.is_overdue() {
                    match watchdog.component_type {
                        ComponentType::LLMInference => {
                            // Restart inference engine
                            self.restart_inference().await;
                        }
                        ComponentType::AgentWorker => {
                            // Restart agent worker
                            self.restart_agent(watchdog.id).await;
                        }
                        ComponentType::CheckpointManager => {
                            // Critical - trigger full recovery
                            self.trigger_full_recovery().await;
                        }
                    }
                }
            }
        }
    }
}
```

---

## 4. Resource Management

### 4.1 GPU Resource Manager

```rust
// resource/gpu.rs
use nvml_wrapper::Nvml;

pub struct GpuManager {
    nvml: Nvml,
    devices: Vec<GpuDevice>,
    allocation_policy: GpuAllocationPolicy,
}

#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub index: u32,
    pub uuid: String,
    pub memory_total: u64,
    pub memory_allocated: Arc<AtomicU64>,
    pub compute_contexts: Arc<AtomicU32>,
}

#[derive(Debug, Clone)]
pub struct GpuAllocation {
    pub device_index: u32,
    pub memory_bytes: u64,
    pub compute_percentage: u32,
}

impl GpuManager {
    /// Monitor GPU metrics continuously
    pub async fn monitor_loop(&self) {
        loop {
            for device in &self.devices {
                let stats = self.get_device_stats(device.index).await;
                
                // Check for issues
                if stats.temperature > 85 {
                    tracing::warn!("GPU {} overheating: {}°C", device.index, stats.temperature);
                    self.throttle_compute(device.index, 0.5).await;
                }
                
                if stats.memory_utilization > 95 {
                    tracing::warn!("GPU {} memory saturated", device.index);
                    self.trigger_model_offload(device.index).await;
                }
                
                // Record metrics
                metrics::gauge!("gpu.temperature", stats.temperature as f64,
                    "device" => device.index.to_string());
                metrics::gauge!("gpu.memory.used", stats.memory_used as f64,
                    "device" => device.index.to_string());
            }
            
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
    
    /// Dynamically adjust model quantization based on available memory
    pub async fn adjust_quantization(&self, required_memory: u64) -> QuantizationLevel {
        let available = self.get_total_available_memory().await;
        
        if available > required_memory * 2 {
            QuantizationLevel::None // Full precision
        } else if available > required_memory * 1.5 {
            QuantizationLevel::FP8
        } else if available > required_memory {
            QuantizationLevel::Int8
        } else if available > required_memory * 0.6 {
            QuantizationLevel::Int4
        } else {
            // Need to unload other models
            self.unload_non_critical_models().await;
            QuantizationLevel::Int4
        }
    }
}
```

### 4.2 Memory Pressure Handler

```rust
// resource/memory.rs
use sysinfo::{System, SystemExt, ProcessExt};

pub struct MemoryManager {
    system: Arc<RwLock<System>>,
    pressure_thresholds: PressureThresholds,
    action_queue: mpsc::Sender<MemoryAction>,
}

#[derive(Clone)]
pub struct PressureThresholds {
    pub warning: f32,  // 70%
    pub critical: f32, // 85%
    pub emergency: f32, // 95%
}

#[derive(Debug, Clone)]
pub enum MemoryAction {
    /// Trigger garbage collection
    RunGC,
    /// Flush caches
    FlushCaches,
    /// Offload model to CPU/disk
    OffloadModel { model_id: String, target: OffloadTarget },
    /// Reduce context window
    ReduceContext { target_tokens: usize },
    /// Pause non-critical tasks
    PauseTasks { priority_threshold: Priority },
    /// Emergency: kill and restart
    EmergencyRestart,
}

impl MemoryManager {
    pub async fn monitor(&self) {
        loop {
            let usage = self.get_memory_usage().await;
            
            if usage > self.pressure_thresholds.emergency {
                self.handle_emergency().await;
            } else if usage > self.pressure_thresholds.critical {
                self.handle_critical().await;
            } else if usage > self.pressure_thresholds.warning {
                self.handle_warning().await;
            }
            
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    
    async fn handle_critical(&self) {
        tracing::warn!("Critical memory pressure detected");
        
        // Ordered by impact (least to most)
        let actions = vec![
            MemoryAction::FlushCaches,
            MemoryAction::ReduceContext { target_tokens: 32768 },
            MemoryAction::OffloadModel { 
                model_id: "secondary".to_string(), 
                target: OffloadTarget::CPU 
            },
            MemoryAction::PauseTasks { priority_threshold: Priority::Low },
        ];
        
        for action in actions {
            let _ = self.action_queue.send(action).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            if self.get_memory_usage().await < self.pressure_thresholds.warning {
                return;
            }
        }
    }
}
```

### 4.3 Disk Space Management

```rust
// resource/disk.rs
pub struct DiskManager {
    checkpoints_path: PathBuf,
    logs_path: PathBuf,
    models_path: PathBuf,
    retention_policy: RetentionPolicy,
}

#[derive(Clone)]
pub struct RetentionPolicy {
    /// Keep checkpoints for N days
    pub checkpoint_retention_days: u32,
    /// Keep logs for N days
    pub log_retention_days: u32,
    /// Max disk usage percentage
    pub max_disk_usage: f32,
    /// Compress checkpoints older than N days
    pub compress_after_days: u32,
}

impl DiskManager {
    pub async fn maintenance_loop(&self) {
        loop {
            self.enforce_retention().await;
            self.compress_old_checkpoints().await;
            self.cleanup_orphaned_files().await;
            
            tokio::time::sleep(Duration::from_secs(3600)).await; // Hourly
        }
    }
    
    async fn enforce_retention(&self) {
        let usage = self.get_disk_usage().await;
        
        if usage > self.retention_policy.max_disk_usage {
            // Aggressive cleanup
            self.delete_old_checkpoints(
                self.retention_policy.checkpoint_retention_days / 2
            ).await;
            
            self.delete_old_logs(
                self.retention_policy.log_retention_days / 2
            ).await;
        }
    }
    
    /// Estimate storage needs for multi-day run
    pub fn estimate_storage_needs(&self, days: u32) -> StorageEstimate {
        let daily_checkpoint_size = 500 * 1024 * 1024u64; // 500MB/day
        let daily_log_size = 100 * 1024 * 1024u64; // 100MB/day
        
        StorageEstimate {
            checkpoints: daily_checkpoint_size * days as u64,
            logs: daily_log_size * days as u64,
            models: self.get_models_size(),
            buffer: daily_checkpoint_size * 2, // 2-day buffer
        }
    }
}
```

### 4.4 Resource Quotas and Limits

```rust
// resource/quotas.rs
pub struct ResourceQuotas {
    /// Max GPU memory per model
    pub max_gpu_memory_per_model: u64,
    /// Max concurrent inference requests
    pub max_concurrent_requests: usize,
    /// Max context window size
    pub max_context_tokens: usize,
    /// Max tasks in queue
    pub max_queued_tasks: usize,
    /// Max disk per checkpoint
    pub max_checkpoint_size: u64,
}

impl Default for ResourceQuotas {
    fn default() -> Self {
        Self {
            max_gpu_memory_per_model: 20 * 1024 * 1024 * 1024, // 20GB
            max_concurrent_requests: 4,
            max_context_tokens: 1_000_000, // Qwen3 1M context
            max_queued_tasks: 1000,
            max_checkpoint_size: 2 * 1024 * 1024 * 1024, // 2GB
        }
    }
}

/// Runtime-adjustable quotas based on system state
pub struct AdaptiveQuotas {
    base: ResourceQuotas,
    current: RwLock<ResourceQuotas>,
}

impl AdaptiveQuotas {
    pub async fn adjust_for_pressure(&self, pressure: ResourcePressure) {
        let mut current = self.current.write().await;
        
        match pressure {
            ResourcePressure::None => {
                *current = self.base.clone();
            }
            ResourcePressure::Low => {
                current.max_concurrent_requests = self.base.max_concurrent_requests - 1;
            }
            ResourcePressure::Medium => {
                current.max_concurrent_requests = self.base.max_concurrent_requests / 2;
                current.max_context_tokens = self.base.max_context_tokens / 2;
            }
            ResourcePressure::High => {
                current.max_concurrent_requests = 1;
                current.max_context_tokens = 32768;
                current.max_queued_tasks = 100;
            }
        }
    }
}
```

---

## 5. LLM Inference Optimization

### 5.1 Model Lifecycle Manager

```rust
// llm/model_manager.rs
pub struct ModelLifecycleManager {
    models: Arc<RwLock<HashMap<ModelId, ModelInstance>>>,
    gpu_manager: Arc<GpuManager>,
    cache_manager: CacheManager,
}

#[derive(Debug, Clone)]
pub struct ModelInstance {
    pub id: ModelId,
    pub state: ModelState,
    pub config: ModelConfig,
    pub engine: Arc<dyn InferenceEngine>,
    pub last_used: Instant,
    pub use_count: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelState {
    Loading,
    Ready,
    Unloading,
    Offloaded, // On CPU/disk
    Error,
}

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model_path: PathBuf,
    pub quantization: QuantizationLevel,
    pub tensor_parallel_size: usize,
    pub gpu_memory_utilization: f32,
    pub max_model_len: usize,
    pub enable_prefix_caching: bool,
}

impl ModelLifecycleManager {
    /// Load model with automatic quantization selection
    pub async fn load_model(&self, config: ModelConfig) -> Result<ModelId, ModelError> {
        let model_id = ModelId::new();
        
        // Check available memory
        let available = self.gpu_manager.get_available_memory().await;
        let required = self.estimate_memory(&config).await;
        
        let adjusted_config = if available < required {
            // Auto-adjust quantization
            let quant = self.gpu_manager
                .adjust_quantization(required)
                .await;
            
            ModelConfig {
                quantization: quant,
                ..config
            }
        } else {
            config
        };
        
        // Load model
        let engine = self.create_engine(&adjusted_config).await?;
        
        let instance = ModelInstance {
            id: model_id.clone(),
            state: ModelState::Ready,
            config: adjusted_config,
            engine: Arc::new(engine),
            last_used: Instant::now(),
            use_count: AtomicU64::new(0),
        };
        
        self.models.write().await.insert(model_id.clone(), instance);
        
        Ok(model_id)
    }
    
    /// Unload least recently used model when memory needed
    pub async fn unload_lru(&self, required_bytes: u64) -> Result<(), ModelError> {
        let models = self.models.read().await;
        
        // Find LRU model that's not critical
        let lru: Option<ModelId> = models
            .values()
            .filter(|m| !m.config.critical)
            .min_by_key(|m| m.last_used)
            .map(|m| m.id.clone());
        
        drop(models);
        
        if let Some(id) = lru {
            self.unload_model(&id).await?;
            Ok(())
        } else {
            Err(ModelError::NoUnloadableModels)
        }
    }
    
    /// Smart model swapping for multi-model scenarios
    pub async fn swap_model(&self, from: &ModelId, to_config: ModelConfig) -> Result<ModelId, ModelError> {
        // Pre-load new model weights to CPU
        let cpu_cache = self.cache_manager.preload_to_cpu(&to_config).await?;
        
        // Unload old model
        self.unload_model(from).await?;
        
        // Quick GPU load from CPU cache
        self.load_from_cpu_cache(cpu_cache).await
    }
}
```

### 5.2 vLLM Integration

```rust
// llm/vllm.rs
use vllm::{LLMEngine, SamplingParams, RequestOutput};

pub struct VllmEngine {
    engine: LLMEngine,
    request_queue: Arc<RwLock<Vec<QueuedRequest>>>,
    batch_scheduler: BatchScheduler,
}

#[derive(Clone)]
pub struct VllmConfig {
    pub model: String,
    pub quantization: Option<String>, // "awq", "gptq", "fp8"
    pub tensor_parallel_size: usize,
    pub gpu_memory_utilization: f32,
    pub max_num_seqs: usize,
    pub max_model_len: usize,
    pub enable_prefix_caching: bool,
    pub enable_chunked_prefill: bool,
}

impl VllmEngine {
    pub async fn new(config: VllmConfig) -> Result<Self, VllmError> {
        let engine = LLMEngine::new(vllm::EngineArgs {
            model: config.model,
            quantization: config.quantization,
            tensor_parallel_size: config.tensor_parallel_size,
            gpu_memory_utilization: config.gpu_memory_utilization,
            max_num_seqs: config.max_num_seqs,
            max_model_len: config.max_model_len,
            enable_prefix_caching: config.enable_prefix_caching,
            enable_chunked_prefill: config.enable_chunked_prefill,
            ..Default::default()
        }).await?;
        
        Ok(Self {
            engine,
            request_queue: Arc::new(RwLock::new(Vec::new())),
            batch_scheduler: BatchScheduler::new(),
        })
    }
    
    /// Optimized generation for long runs
    pub async fn generate_stream(
        &self,
        prompt: String,
        params: GenerationParams,
    ) -> impl Stream<Item = Result<TokenOutput, VllmError>> {
        let sampling_params = SamplingParams {
            temperature: params.temperature,
            max_tokens: params.max_tokens,
            top_p: params.top_p,
            ..Default::default()
        };
        
        // Use prefix caching for repeated prompts
        let prefix_id = if params.enable_caching {
            Some(self.compute_prefix_id(&prompt))
        } else {
            None
        };
        
        self.engine.generate_stream(prompt, sampling_params, prefix_id)
    }
    
    /// Continuous batching for multiple agents
    pub async fn run_batch_scheduler(&self) {
        loop {
            let mut queue = self.request_queue.write().await;
            
            // Dynamic batching based on current load
            let batch_size = self.compute_optimal_batch_size().await;
            let batch: Vec<_> = queue.drain(..batch_size.min(queue.len())).collect();
            
            drop(queue);
            
            if !batch.is_empty() {
                self.process_batch(batch).await;
            }
            
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
```

### 5.3 Context Window Management

```rust
// llm/context.rs
pub struct ContextWindowManager {
    max_tokens: usize,
    compression_strategy: CompressionStrategy,
    summarizer: Arc<dyn Summarizer>,
}

#[derive(Clone)]
pub enum CompressionStrategy {
    /// Sliding window - keep only recent tokens
    SlidingWindow { window_size: usize },
    /// Hierarchical - summarize older content
    Hierarchical { summary_interval: usize },
    /// Selective - keep important messages
    Selective { importance_threshold: f32 },
    /// Hybrid - combine strategies
    Hybrid(Box<CompressionStrategy>, Box<CompressionStrategy>),
}

impl ContextWindowManager {
    /// Compress context when approaching limit
    pub async fn compress_if_needed(
        &self,
        context: &mut ConversationContext,
    ) -> Result<(), ContextError> {
        let token_count = context.estimate_tokens();
        
        if token_count > self.max_tokens * 9 / 10 {
            tracing::info!("Compressing context: {} tokens", token_count);
            
            match &self.compression_strategy {
                CompressionStrategy::Hierarchical { summary_interval } => {
                    self.hierarchical_compress(context, *summary_interval).await?;
                }
                CompressionStrategy::SlidingWindow { window_size } => {
                    self.sliding_window_compress(context, *window_size);
                }
                CompressionStrategy::Selective { importance_threshold } => {
                    self.selective_compress(context, *importance_threshold).await?;
                }
                CompressionStrategy::Hybrid(a, b) => {
                    // Apply both strategies
                    self.apply_strategy(context, a).await?;
                    if context.estimate_tokens() > self.max_tokens * 8 / 10 {
                        self.apply_strategy(context, b).await?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn hierarchical_compress(
        &self,
        context: &mut ConversationContext,
        interval: usize,
    ) -> Result<(), ContextError> {
        // Group messages into chunks
        let chunks: Vec<_> = context.messages
            .chunks(interval)
            .map(|c| c.to_vec())
            .collect();
        
        // Summarize older chunks
        let mut new_messages = Vec::new();
        
        for (i, chunk) in chunks.iter().enumerate() {
            if i == chunks.len() - 1 {
                // Keep most recent chunk intact
                new_messages.extend(chunk.clone());
            } else {
                // Summarize older chunks
                let summary = self.summarizer.summarize(chunk).await?;
                new_messages.push(Message::system(format!(
                    "[Summary of previous {} messages]: {}",
                    chunk.len(),
                    summary
                )));
            }
        }
        
        context.messages = new_messages;
        Ok(())
    }
}
```

### 5.4 Request Queue and Prioritization

```rust
// llm/queue.rs
pub struct InferenceQueue {
    high_priority: VecDeque<InferenceRequest>,
    normal_priority: VecDeque<InferenceRequest>,
    low_priority: VecDeque<InferenceRequest>,
    in_progress: HashMap<RequestId, InProgressRequest>,
    max_concurrent: usize,
}

#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub id: RequestId,
    pub prompt: String,
    pub params: GenerationParams,
    pub priority: Priority,
    pub deadline: Option<Instant>,
    pub estimated_tokens: usize,
    pub checkpoint_on_completion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 0,  // System recovery, checkpoints
    High = 1,      // User-facing, time-sensitive
    Normal = 2,    // Standard agent work
    Low = 3,       // Background optimization
    Background = 4, // Self-improvement, analysis
}

impl InferenceQueue {
    pub fn enqueue(&mut self, request: InferenceRequest) {
        match request.priority {
            Priority::Critical | Priority::High => {
                self.high_priority.push_back(request);
            }
            Priority::Normal => {
                self.normal_priority.push_back(request);
            }
            Priority::Low | Priority::Background => {
                self.low_priority.push_back(request);
            }
        }
    }
    
    pub fn next(&mut self) -> Option<InferenceRequest> {
        // Check deadlines first
        for queue in [&mut self.high_priority, &mut self.normal_priority] {
            if let Some(pos) = queue.iter().position(|r| {
                r.deadline.map(|d| d < Instant::now()).unwrap_or(false)
            }) {
                return queue.remove(pos);
            }
        }
        
        // Normal priority ordering
        self.high_priority.pop_front()
            .or_else(|| self.normal_priority.pop_front())
            .or_else(|| self.low_priority.pop_front())
    }
    
    /// Preempt low-priority requests for critical ones
    pub async fn preempt_if_needed(&mut self, critical_request: &InferenceRequest) -> bool {
        if self.in_progress.len() >= self.max_concurrent {
            // Find lowest priority in-progress request
            if let Some((id, _)) = self.in_progress
                .iter()
                .min_by_key(|(_, r)| r.priority)
            {
                if self.in_progress[id].priority > critical_request.priority {
                    self.preempt(id.clone()).await;
                    return true;
                }
            }
        }
        false
    }
}
```

---

## 6. Log Aggregation and Observability

### 6.1 Structured Logging

```rust
// observability/logging.rs
use tracing::{info, warn, error, debug, span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct StructuredLogger {
    file_appender: RollingFileAppender,
    json_layer: JsonLayer,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub span_context: Vec<SpanContext>,
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub session_id: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
}

impl StructuredLogger {
    pub fn init(config: &LoggingConfig) -> Result<(), LogError> {
        // File appender with rotation
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("selfware")
            .filename_suffix("log")
            .max_log_files(config.retention_days as usize)
            .build(&config.log_dir)?;
        
        // JSON formatting for structured logs
        let json_layer = fmt::layer()
            .json()
            .with_writer(file_appender)
            .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
            .with_current_span(true)
            .with_span_list(true);
        
        // Console layer for development
        let console_layer = fmt::layer()
            .pretty()
            .with_filter(EnvFilter::from_default_env());
        
        tracing_subscriber::registry()
            .with(json_layer)
            .with(console_layer)
            .with(MetricsLayer::new())
            .init();
        
        Ok(())
    }
}

/// Agent-specific tracing span
pub fn agent_span(agent_id: &str, task_id: Option<&str>) -> tracing::Span {
    tracing::info_span!(
        "agent",
        agent_id = agent_id,
        task_id = task_id.unwrap_or("none"),
        session_id = SESSION_ID.get().unwrap_or("unknown"),
    )
}

/// Task lifecycle logging
pub fn log_task_event(task_id: &str, event: TaskEvent) {
    match event {
        TaskEvent::Started { task_type } => {
            info!(
                task_id = task_id,
                task_type = task_type,
                "Task started"
            );
        }
        TaskEvent::Checkpointed { checkpoint_id } => {
            info!(
                task_id = task_id,
                checkpoint_id = checkpoint_id,
                "Task checkpointed"
            );
        }
        TaskEvent::Completed { duration, tokens_used } => {
            info!(
                task_id = task_id,
                duration_ms = duration.as_millis(),
                tokens_used = tokens_used,
                "Task completed"
            );
        }
        TaskEvent::Failed { error, recoverable } => {
            if recoverable {
                warn!(
                    task_id = task_id,
                    error = %error,
                    "Task failed (recoverable)"
                );
            } else {
                error!(
                    task_id = task_id,
                    error = %error,
                    "Task failed (unrecoverable)"
                );
            }
        }
    }
}
```

### 6.2 Metrics Collection

```rust
// observability/metrics.rs
use metrics::{counter, gauge, histogram, describe_counter, describe_gauge};
use metrics_exporter_prometheus::PrometheusBuilder;

pub struct MetricsCollector {
    registry: Registry,
}

impl MetricsCollector {
    pub fn init(prometheus_port: u16) -> Result<Self, MetricsError> {
        PrometheusBuilder::new()
            .with_http_listener(([0, 0, 0, 0], prometheus_port))
            .install_recorder()?;
        
        // Describe metrics
        describe_counter!(
            "selfware_tasks_completed_total",
            "Total number of completed tasks"
        );
        describe_counter!(
            "selfware_tasks_failed_total",
            "Total number of failed tasks"
        );
        describe_gauge!(
            "selfware_active_agents",
            "Number of currently active agents"
        );
        describe_gauge!(
            "selfware_context_tokens",
            "Current context window token count"
        );
        describe_gauge!(
            "selfware_gpu_memory_used_bytes",
            "GPU memory used in bytes"
        );
        describe_histogram!(
            "selfware_inference_duration_seconds",
            "Inference request duration"
        );
        describe_histogram!(
            "selfware_checkpoint_size_bytes",
            "Size of checkpoints in bytes"
        );
        
        Ok(Self { registry: Registry::new() })
    }
    
    pub fn record_task_completion(&self, task: &CompletedTask) {
        counter!("selfware_tasks_completed_total",
            "task_type" => task.task_type.clone()
        ).increment(1);
        
        histogram!(
            "selfware_task_duration_seconds",
            "task_type" => task.task_type.clone()
        ).record(task.duration.as_secs_f64());
        
        histogram!(
            "selfware_task_tokens_used",
            "task_type" => task.task_type.clone()
        ).record(task.tokens_used as f64);
    }
    
    pub fn update_system_metrics(&self, state: &SystemState) {
        gauge!("selfware_active_agents").set(state.active_agents as f64);
        gauge!("selfware_queued_tasks").set(state.queued_tasks as f64);
        gauge!("selfware_context_tokens").set(state.context_tokens as f64);
        gauge!("selfware_memory_used_bytes").set(state.memory_used as f64);
        gauge!("selfware_gpu_memory_used_bytes").set(state.gpu_memory_used as f64);
    }
}
```

### 6.3 Distributed Tracing

```rust
// observability/tracing.rs
use opentelemetry::trace::{Tracer, TraceContextExt};
use opentelemetry_otlp::WithExportConfig;

pub struct TracingConfig {
    pub jaeger_endpoint: Option<String>,
    pub sampling_rate: f64,
    pub max_events_per_span: u32,
}

pub fn init_tracing(config: &TracingConfig) -> Result<TracerProvider, TraceError> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(config.jaeger_endpoint.as_deref().unwrap_or("http://localhost:4317"))
        )
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(Sampler::TraceIdRatioBased(config.sampling_rate))
                .with_max_events_per_span(config.max_events_per_span),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;
    
    // Create tracing layer
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer.clone());
    
    tracing_subscriber::registry()
        .with(telemetry)
        .init();
    
    Ok(tracer)
}

/// Create a span for long-running operations
pub fn long_running_span(name: &str, operation_id: &str) -> tracing::Span {
    tracing::info_span!(
        name,
        operation_id = operation_id,
        start_time = chrono::Utc::now().to_rfc3339(),
    )
}

/// Track async operation with timeout
pub async fn track_with_timeout<F, T>(
    operation: F,
    timeout: Duration,
    span: &tracing::Span,
) -> Result<T, TimeoutError>
where
    F: std::future::Future<Output = T>,
{
    let _enter = span.enter();
    
    match tokio::time::timeout(timeout, operation).await {
        Ok(result) => {
            tracing::debug!("Operation completed within timeout");
            Ok(result)
        }
        Err(_) => {
            tracing::error!("Operation timed out after {:?}", timeout);
            Err(TimeoutError)
        }
    }
}
```

### 6.4 Health Check Endpoint

```rust
// observability/health.rs
use axum::{Router, routing::get, Json};
use serde_json::json;

pub struct HealthServer {
    checks: Vec<Box<dyn HealthCheck + Send + Sync>>,
}

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub status: OverallStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: u64,
    pub checks: Vec<ComponentHealth>,
    pub version: String,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: Status,
    pub message: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub response_time_ms: u64,
}

impl HealthServer {
    pub fn router() -> Router {
        Router::new()
            .route("/health", get(health_handler))
            .route("/health/ready", get(readiness_handler))
            .route("/health/live", get(liveness_handler))
            .route("/metrics", get(metrics_handler))
    }
}

async fn health_handler() -> Json<HealthReport> {
    let checks = vec![
        check_inference_engine().await,
        check_checkpoint_store().await,
        check_gpu_availability().await,
        check_disk_space().await,
    ];
    
    let overall = if checks.iter().all(|c| c.status == Status::Healthy) {
        OverallStatus::Healthy
    } else if checks.iter().any(|c| c.status == Status::Unhealthy) {
        OverallStatus::Unhealthy
    } else {
        OverallStatus::Degraded
    };
    
    Json(HealthReport {
        status: overall,
        timestamp: chrono::Utc::now(),
        uptime_seconds: get_uptime(),
        checks,
        version: env!("CARGO_PKG_VERSION").to_string(),
        session_id: get_session_id(),
    })
}

async fn check_inference_engine() -> ComponentHealth {
    let start = Instant::now();
    
    match ping_inference_engine().await {
        Ok(_) => ComponentHealth {
            name: "inference_engine".to_string(),
            status: Status::Healthy,
            message: None,
            last_check: chrono::Utc::now(),
            response_time_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => ComponentHealth {
            name: "inference_engine".to_string(),
            status: Status::Unhealthy,
            message: Some(e.to_string()),
            last_check: chrono::Utc::now(),
            response_time_ms: start.elapsed().as_millis() as u64,
        },
    }
}
```

---

## 7. Human Intervention Points

### 7.1 Intervention API

```rust
// intervention/api.rs
use axum::{Router, Json, extract::State};

pub struct InterventionServer {
    state: Arc<InterventionState>,
    notifier: NotificationChannel,
}

#[derive(Debug, Clone)]
pub struct InterventionState {
    pub pending_approvals: Arc<RwLock<Vec<ApprovalRequest>>>,
    pub system_pause: Arc<RwLock<bool>>,
    pub intervention_history: Arc<RwLock<Vec<InterventionRecord>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub request_type: ApprovalType,
    pub description: String,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    pub context: serde_json::Value,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalType {
    /// Code execution approval
    CodeExecution { code: String, language: String },
    /// File system modification
    FileModification { path: String, operation: String },
    /// Network request
    NetworkRequest { url: String, method: String },
    /// System command
    SystemCommand { command: String, args: Vec<String> },
    /// Self-modification (recursive improvement)
    SelfModification { component: String, changes: String },
    /// Resource threshold override
    ResourceOverride { resource: String, requested: u64 },
}

impl InterventionServer {
    pub fn router(state: Arc<InterventionState>) -> Router {
        Router::new()
            .route("/interventions", get(list_interventions))
            .route("/interventions/:id/approve", post(approve_intervention))
            .route("/interventions/:id/reject", post(reject_intervention))
            .route("/pause", post(pause_system))
            .route("/resume", post(resume_system))
            .route("/status", get(system_status))
            .with_state(state)
    }
    
    /// Request approval for sensitive operation
    pub async fn request_approval(
        &self,
        request_type: ApprovalType,
    ) -> Result<ApprovalResponse, InterventionError> {
        let request = ApprovalRequest {
            id: generate_id(),
            request_type,
            description: self.describe_request(&request_type),
            requested_at: chrono::Utc::now(),
            timeout_at: Some(chrono::Utc::now() + chrono::Duration::minutes(30)),
            context: self.gather_context(),
            status: ApprovalStatus::Pending,
        };
        
        // Store request
        self.state.pending_approvals.write().await.push(request.clone());
        
        // Notify human
        self.notifier.send(Notification::ApprovalRequired(request.clone())).await?;
        
        // Wait for response with timeout
        match self.wait_for_response(&request.id, Duration::from_secs(1800)).await {
            Some(response) => Ok(response),
            None => {
                // Timeout - apply default policy
                self.handle_timeout(&request).await
            }
        }
    }
}
```

### 7.2 Intervention Policies

```rust
// intervention/policies.rs
pub struct InterventionPolicy {
    /// Auto-approve after timeout
    pub default_on_timeout: DefaultAction,
    /// Require approval for these operations
    pub require_approval: Vec<ApprovalTypePattern>,
    /// Auto-approve these operations
    pub auto_approve: Vec<ApprovalTypePattern>,
    /// Auto-reject these operations
    pub auto_reject: Vec<ApprovalTypePattern>,
    /// Notification channels
    pub notifications: NotificationConfig,
}

#[derive(Debug, Clone)]
pub enum DefaultAction {
    Approve,
    Reject,
    Escalate,
    Pause,
}

impl Default for InterventionPolicy {
    fn default() -> Self {
        Self {
            default_on_timeout: DefaultAction::Pause,
            require_approval: vec![
                ApprovalTypePattern::SelfModification { .. },
                ApprovalTypePattern::SystemCommand { .. },
                ApprovalTypePattern::NetworkRequest { 
                    url_pattern: Regex::new(r".*external.*").unwrap(),
                },
            ],
            auto_approve: vec![
                ApprovalTypePattern::FileModification {
                    path_pattern: Regex::new(r"/tmp/.*").unwrap(),
                },
            ],
            auto_reject: vec![
                ApprovalTypePattern::SystemCommand {
                    command_pattern: Regex::new(r"rm\s+-rf\s+/").unwrap(),
                },
            ],
            notifications: NotificationConfig::default(),
        }
    }
}

impl InterventionPolicy {
    /// Evaluate operation against policy
    pub fn evaluate(&self, request: &ApprovalRequest) -> PolicyDecision {
        // Check auto-reject first
        for pattern in &self.auto_reject {
            if pattern.matches(&request.request_type) {
                return PolicyDecision::Reject("Matches auto-reject pattern".to_string());
            }
        }
        
        // Check auto-approve
        for pattern in &self.auto_approve {
            if pattern.matches(&request.request_type) {
                return PolicyDecision::Approve;
            }
        }
        
        // Check require approval
        for pattern in &self.require_approval {
            if pattern.matches(&request.request_type) {
                return PolicyDecision::RequireApproval;
            }
        }
        
        // Default
        PolicyDecision::RequireApproval
    }
}
```

### 7.3 Progress Dashboard

```rust
// intervention/dashboard.rs
pub struct DashboardServer {
    state: Arc<SystemState>,
}

#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub session_info: SessionInfo,
    pub current_tasks: Vec<TaskSummary>,
    pub resource_usage: ResourceUsage,
    pub recent_events: Vec<SystemEvent>,
    pub checkpoint_status: CheckpointStatus,
    pub agent_status: Vec<AgentStatus>,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub duration: chrono::Duration,
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
    pub current_goal: String,
    pub progress_percentage: f32,
}

impl DashboardServer {
    pub async fn get_dashboard_data(&self) -> DashboardData {
        DashboardData {
            session_info: self.get_session_info().await,
            current_tasks: self.get_current_tasks().await,
            resource_usage: self.get_resource_usage().await,
            recent_events: self.get_recent_events(50).await,
            checkpoint_status: self.get_checkpoint_status().await,
            agent_status: self.get_agent_status().await,
        }
    }
    
    pub fn websocket_handler(&self) -> impl Stream<Item = DashboardUpdate> {
        // Real-time updates via WebSocket
        tokio_stream::wrappers::BroadcastStream::new(self.state.update_channel.subscribe())
            .filter_map(|result| result.ok())
    }
}
```

---

## 8. Rust Implementation

### 8.1 Recommended Crate Stack

| Category | Crate | Version | Purpose |
|----------|-------|---------|---------|
| **Async Runtime** | tokio | ^1.35 | Async runtime, tasks, channels |
| **Serialization** | serde | ^1.0 | Data serialization |
| **Binary** | bincode | ^1.3 | Fast binary serialization |
| **Compression** | zstd | ^0.13 | Checkpoint compression |
| **Storage** | sled | ^0.34 | Embedded database |
| **HTTP** | axum | ^0.7 | Web server, health checks |
| **Metrics** | metrics + metrics-exporter-prometheus | ^0.23 | Metrics collection |
| **Tracing** | tracing + tracing-subscriber | ^0.1 | Structured logging |
| **OpenTelemetry** | opentelemetry-otlp | ^0.15 | Distributed tracing |
| **GPU** | nvml-wrapper | ^0.10 | NVIDIA GPU monitoring |
| **System** | sysinfo | ^0.30 | System resource monitoring |
| **Time** | chrono | ^0.4 | Date/time handling |
| **UUID** | uuid | ^1.7 | Unique identifiers |
| **Regex** | regex | ^1.10 | Pattern matching |
| **Error Handling** | thiserror | ^1.0 | Error definitions |
| **Configuration** | config | ^0.14 | Configuration management |
| **CLI** | clap | ^4.5 | Command-line parsing |
| **vLLM** | vllm-rs (custom) | - | LLM inference |

### 8.2 Core Module Structure

```
selfware/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs              # Configuration management
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── worker.rs          # Agent worker implementation
│   │   ├── supervisor.rs      # Agent supervision
│   │   ├── state.rs           # Agent state management
│   │   └── checkpointing.rs   # Task-level checkpointing
│   ├── checkpoint/
│   │   ├── mod.rs
│   │   ├── storage.rs         # Checkpoint storage
│   │   ├── incremental.rs     # Incremental checkpoints
│   │   ├── scheduler.rs       # Checkpoint scheduling
│   │   ├── recovery.rs        # Recovery protocols
│   │   └── compression.rs     # Checkpoint compression
│   ├── supervision/
│   │   ├── mod.rs
│   │   ├── tree.rs            # Supervision tree
│   │   ├── health.rs          # Health checks
│   │   ├── circuit_breaker.rs # Circuit breakers
│   │   ├── watchdog.rs        # Watchdog timers
│   │   └── restart.rs         # Restart policies
│   ├── resource/
│   │   ├── mod.rs
│   │   ├── gpu.rs             # GPU management
│   │   ├── memory.rs          # Memory management
│   │   ├── disk.rs            # Disk management
│   │   ├── quotas.rs          # Resource quotas
│   │   └── monitor.rs         # Resource monitoring
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── engine.rs          # LLM engine trait
│   │   ├── vllm.rs            # vLLM integration
│   │   ├── ollama.rs          # Ollama integration
│   │   ├── model_manager.rs   # Model lifecycle
│   │   ├── context.rs         # Context management
│   │   ├── queue.rs           # Request queue
│   │   └── tokenizer.rs       # Token counting
│   ├── observability/
│   │   ├── mod.rs
│   │   ├── logging.rs         # Structured logging
│   │   ├── metrics.rs         # Metrics collection
│   │   ├── tracing.rs         # Distributed tracing
│   │   ├── health.rs          # Health endpoints
│   │   └── alerts.rs          # Alerting
│   ├── intervention/
│   │   ├── mod.rs
│   │   ├── api.rs             # Intervention API
│   │   ├── policies.rs        # Intervention policies
│   │   ├── dashboard.rs       # Progress dashboard
│   │   └── notifications.rs   # Notifications
│   ├── self_healing.rs        # Error recovery
│   ├── persistence/
│   │   ├── mod.rs
│   │   ├── journal.rs         # Recovery journal
│   │   └── state_store.rs     # State storage
│   └── utils/
│       ├── mod.rs
│       ├── backoff.rs         # Backoff strategies
│       ├── id.rs              # ID generation
│       └── time.rs            # Time utilities
├── docker/
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── entrypoint.sh
└── config/
    ├── default.toml
    ├── development.toml
    └── production.toml
```

### 8.3 Main Application Entry Point

```rust
// src/main.rs
use selfware::config::Config;
use selfware::observability::{init_logging, init_metrics, init_tracing};
use selfware::supervision::Supervisor;
use selfware::checkpoint::CheckpointManager;
use selfware::resource::ResourceManager;
use selfware::llm::ModelLifecycleManager;
use selfware::intervention::InterventionServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = Config::load()?;
    
    // Initialize observability
    init_logging(&config.logging)?;
    init_metrics(&config.metrics)?;
    init_tracing(&config.tracing)?;
    
    tracing::info!("Starting Selfware autonomous runtime");
    
    // Initialize core components
    let checkpoint_manager = Arc::new(CheckpointManager::new(&config.checkpoint).await?);
    let resource_manager = Arc::new(ResourceManager::new(&config.resources).await?);
    let model_manager = Arc::new(ModelLifecycleManager::new(&config.llm).await?);
    
    // Attempt recovery if needed
    let recovered_state = checkpoint_manager.recover().await?;
    
    // Create supervision tree
    let supervisor = Supervisor::new()
        .with_strategy(SupervisionStrategy::OneForOne)
        .with_restart_policy(RestartPolicy {
            max_restarts: 5,
            max_seconds: 60,
            backoff_strategy: BackoffStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(60),
            },
        })
        .add_child(ChildSpec::new("checkpoint_manager", checkpoint_manager.clone()))
        .add_child(ChildSpec::new("resource_manager", resource_manager.clone()))
        .add_child(ChildSpec::new("model_manager", model_manager.clone()));
    
    // Start supervision tree
    let supervisor_handle = supervisor.start().await?;
    
    // Start resource monitoring
    let resource_monitor = tokio::spawn({
        let rm = resource_manager.clone();
        async move { rm.monitor_loop().await }
    });
    
    // Start checkpoint scheduler
    let checkpoint_scheduler = tokio::spawn({
        let cm = checkpoint_manager.clone();
        async move { cm.scheduler_loop().await }
    });
    
    // Start intervention server
    let intervention_server = InterventionServer::new(&config.intervention).await?;
    let intervention_handle = tokio::spawn(async move {
        let app = intervention_server.router();
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
    
    // Start main agent loop
    let agent_loop = tokio::spawn({
        let cm = checkpoint_manager.clone();
        let mm = model_manager.clone();
        async move { run_agent_loop(recovered_state, cm, mm).await }
    });
    
    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal, initiating graceful shutdown");
        }
        result = agent_loop => {
            match result {
                Ok(_) => tracing::info!("Agent loop completed"),
                Err(e) => tracing::error!("Agent loop failed: {:?}", e),
            }
        }
    }
    
    // Graceful shutdown
    graceful_shutdown(checkpoint_manager).await?;
    
    Ok(())
}

async fn run_agent_loop(
    initial_state: RecoveredState,
    checkpoint_manager: Arc<CheckpointManager>,
    model_manager: Arc<ModelLifecycleManager>,
) -> Result<(), AgentError> {
    let mut state = initial_state.into_agent_state();
    
    loop {
        // Checkpoint before each major iteration
        checkpoint_manager.checkpoint_session(&state).await?;
        
        // Get next task or generate self-improvement task
        let task = match state.next_task().await {
            Some(task) => task,
            None => {
                // Generate self-improvement task
                generate_self_improvement_task(&state).await?
            }
        };
        
        // Execute task with supervision
        match execute_task_with_recovery(&task, &model_manager).await {
            Ok(result) => {
                state.record_completion(task, result);
            }
            Err(e) if e.is_recoverable() => {
                tracing::warn!("Task failed (recoverable): {:?}", e);
                state.requeue_task(task);
            }
            Err(e) => {
                tracing::error!("Task failed (unrecoverable): {:?}", e);
                state.record_failure(task, e);
            }
        }
        
        // Periodic maintenance
        if state.iteration_count % 100 == 0 {
            state = perform_maintenance(state).await?;
        }
    }
}

async fn graceful_shutdown(checkpoint_manager: Arc<CheckpointManager>) -> Result<(), ShutdownError> {
    tracing::info!("Creating final checkpoint before shutdown");
    
    // Create final checkpoint
    checkpoint_manager.create_graceful_shutdown_checkpoint().await?;
    
    // Flush all pending writes
    checkpoint_manager.flush().await?;
    
    tracing::info!("Graceful shutdown complete");
    Ok(())
}
```

---

## 9. Docker/Container Strategy

### 9.1 Dockerfile

```dockerfile
# docker/Dockerfile
FROM nvidia/cuda:12.1-devel-ubuntu22.04 as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    pkg-config \
    libssl-dev \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Build selfware
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# Runtime stage
FROM nvidia/cuda:12.1-runtime-ubuntu22.04

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install vLLM
RUN pip3 install vllm==0.2.7

# Create selfware user
RUN useradd -m -u 1000 selfware

# Copy binary
COPY --from=builder /build/target/release/selfware /usr/local/bin/selfware

# Copy entrypoint
COPY docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Create directories
RUN mkdir -p /data/checkpoints /data/logs /data/models /data/cache && \
    chown -R selfware:selfware /data

USER selfware

# Environment variables
ENV SELFWARE_CONFIG_PATH=/data/config.toml
ENV SELFWARE_CHECKPOINT_DIR=/data/checkpoints
ENV SELFWARE_LOG_DIR=/data/logs
ENV SELFWARE_MODEL_DIR=/data/models
ENV SELFWARE_CACHE_DIR=/data/cache
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:8080/health/live || exit 1

EXPOSE 8080 9090

ENTRYPOINT ["/entrypoint.sh"]
CMD ["selfware"]
```

### 9.2 Entrypoint Script

```bash
#!/bin/bash
# docker/entrypoint.sh

set -e

# Configuration defaults
export SELFWARE_SESSION_ID=${SELFWARE_SESSION_ID:-$(uuidgen)}
export SELFWARE_MAX_RUNTIME_HOURS=${SELFWARE_MAX_RUNTIME_HOURS:-168}  # 7 days
export SELFWARE_CHECKPOINT_INTERVAL=${SELFWARE_CHECKPOINT_INTERVAL:-300}  # 5 min

# GPU configuration
export CUDA_VISIBLE_DEVICES=${CUDA_VISIBLE_DEVICES:-0}
export VLLM_GPU_MEMORY_UTILIZATION=${VLLM_GPU_MEMORY_UTILIZATION:-0.85}

# Recovery mode
if [ "$SELFWARE_RECOVER" = "true" ]; then
    echo "Starting in recovery mode..."
    export SELFWARE_RECOVER_FROM_CHECKPOINT=true
fi

# Pre-flight checks
echo "Running pre-flight checks..."

# Check GPU availability
if ! nvidia-smi > /dev/null 2>&1; then
    echo "ERROR: No GPU detected"
    exit 1
fi

# Check disk space
AVAILABLE_GB=$(df /data | awk 'NR==2 {print int($4/1024/1024)}')
if [ "$AVAILABLE_GB" -lt 50 ]; then
    echo "WARNING: Low disk space (${AVAILABLE_GB}GB available, 50GB recommended)"
fi

# Check model availability
if [ -n "$SELFWARE_MODEL_PATH" ] && [ ! -f "$SELFWARE_MODEL_PATH" ]; then
    echo "ERROR: Model not found at $SELFWARE_MODEL_PATH"
    exit 1
fi

# Setup signal handlers for graceful shutdown
cleanup() {
    echo "Received shutdown signal, waiting for checkpoint..."
    # Signal selfware to create final checkpoint
    kill -USR1 $SELFWARE_PID 2>/dev/null || true
    wait $SELFWARE_PID
    exit 0
}
trap cleanup SIGTERM SIGINT

# Start selfware
echo "Starting Selfware session: $SELFWARE_SESSION_ID"
selfware "$@" &
SELFWARE_PID=$!

# Wait for selfware
wait $SELFWARE_PID
```

### 9.3 Docker Compose Configuration

```yaml
# docker/docker-compose.yml
version: '3.8'

services:
  selfware:
    build:
      context: ..
      dockerfile: docker/Dockerfile
    image: selfware:latest
    container_name: selfware-autonomous
    
    # Runtime configuration
    runtime: nvidia
    
    environment:
      # Session configuration
      - SELFWARE_SESSION_ID=${SElFWARE_SESSION_ID:-}
      - SELFWARE_MAX_RUNTIME_HOURS=168
      
      # Model configuration
      - SELFWARE_MODEL_PATH=/data/models/qwen3-coder-32b
      - VLLM_TENSOR_PARALLEL_SIZE=1
      - VLLM_GPU_MEMORY_UTILIZATION=0.85
      - VLLM_MAX_MODEL_LEN=1000000
      
      # Checkpoint configuration
      - SELFWARE_CHECKPOINT_INTERVAL=300
      - SELFWARE_CHECKPOINT_COMPRESSION=zstd
      - SELFWARE_CHECKPOINT_RETENTION_DAYS=7
      
      # Resource limits
      - SELFWARE_MAX_GPU_MEMORY=20000000000  # 20GB
      - SELFWARE_MAX_CONTEXT_TOKENS=1000000
      
      # Recovery
      - SELFWARE_AUTO_RECOVER=true
      - SELFWARE_MAX_RESTARTS=10
      
      # Observability
      - RUST_LOG=info
      - METRICS_PORT=9090
      - HEALTH_PORT=8080
      
    volumes:
      # Persistent data
      - selfware-data:/data
      
      # Model cache (optional: use volume for faster restarts)
      - selfware-models:/data/models:ro
      
      # Configuration
      - ./config/production.toml:/data/config.toml:ro
      
      # Logs (host mount for easy access)
      - ./logs:/data/logs
      
    ports:
      - "8080:8080"  # Health & intervention API
      - "9090:9090"  # Prometheus metrics
      
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
              
    # Restart policy for resilience
    restart: unless-stopped
    
    # Health check
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health/live"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 60s
      
    # Logging configuration
    logging:
      driver: "json-file"
      options:
        max-size: "100m"
        max-file: "10"
        
    networks:
      - selfware-network

  # Optional: Prometheus for metrics
  prometheus:
    image: prom/prometheus:latest
    container_name: selfware-prometheus
    volumes:
      - ./config/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    ports:
      - "9091:9090"
    networks:
      - selfware-network
    profiles:
      - monitoring

  # Optional: Grafana for dashboards
  grafana:
    image: grafana/grafana:latest
    container_name: selfware-grafana
    volumes:
      - ./config/grafana:/etc/grafana/provisioning:ro
      - grafana-data:/var/lib/grafana
    ports:
      - "3000:3000"
    networks:
      - selfware-network
    profiles:
      - monitoring

volumes:
  selfware-data:
    driver: local
  selfware-models:
    driver: local
  prometheus-data:
    driver: local
  grafana-data:
    driver: local

networks:
  selfware-network:
    driver: bridge
```

### 9.4 Kubernetes Deployment (Optional)

```yaml
# k8s/selfware-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: selfware-autonomous
  labels:
    app: selfware
spec:
  replicas: 1
  strategy:
    type: Recreate  # Only one instance at a time
  selector:
    matchLabels:
      app: selfware
  template:
    metadata:
      labels:
        app: selfware
    spec:
      runtimeClassName: nvidia
      containers:
        - name: selfware
          image: selfware:latest
          resources:
            limits:
              nvidia.com/gpu: 1
              memory: "64Gi"
              cpu: "16"
            requests:
              memory: "32Gi"
              cpu: "8"
          env:
            - name: SELFWARE_SESSION_ID
              valueFrom:
                fieldRef:
                  fieldPath: metadata.name
            - name: SELFWARE_MAX_RUNTIME_HOURS
              value: "168"
            - name: VLLM_GPU_MEMORY_UTILIZATION
              value: "0.85"
          ports:
            - containerPort: 8080
              name: http
            - containerPort: 9090
              name: metrics
          volumeMounts:
            - name: data
              mountPath: /data
            - name: models
              mountPath: /data/models
              readOnly: true
            - name: config
              mountPath: /data/config.toml
              subPath: config.toml
          livenessProbe:
            httpGet:
              path: /health/live
              port: 8080
            initialDelaySeconds: 60
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /health/ready
              port: 8080
            initialDelaySeconds: 30
            periodSeconds: 10
          lifecycle:
            preStop:
              exec:
                command: ["/bin/sh", "-c", "sleep 30"]  # Time for checkpoint
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: selfware-data
        - name: models
          persistentVolumeClaim:
            claimName: selfware-models
        - name: config
          configMap:
            name: selfware-config
```

---

## 10. Configuration Example

```toml
# config/production.toml
[system]
session_id = "auto"  # Auto-generate
max_runtime_hours = 168  # 7 days
shutdown_grace_period_seconds = 60

[checkpoint]
enabled = true
interval_seconds = 300  # 5 minutes
compression = "zstd"
compression_level = 6
storage_path = "/data/checkpoints"
max_size_bytes = 10_737_418_240  # 10GB
retention_days = 7
incremental = true
content_defined_chunking = true

[checkpoint.levels]
micro = { enabled = true, interval_seconds = 30 }
task = { enabled = true, on_completion = true }
session = { enabled = true, interval_seconds = 300 }
system = { enabled = true, interval_seconds = 900 }

[supervision]
strategy = "OneForOne"
max_restarts = 5
max_seconds = 60
backoff_strategy = { type = "Exponential", base_seconds = 1, max_seconds = 60 }

[supervision.health_check]
interval_seconds = 10
timeout_seconds = 5
failure_threshold = 3

[resources]
[resources.gpu]
monitor_interval_seconds = 5
temperature_threshold = 85
memory_utilization_threshold = 0.95
throttle_on_overheat = true

[resources.memory]
warning_threshold = 0.70
critical_threshold = 0.85
emergency_threshold = 0.95
monitor_interval_seconds = 2

[resources.disk]
max_usage_percent = 0.85
maintenance_interval_seconds = 3600
compress_after_days = 1

[resources.quotas]
max_gpu_memory_per_model = 20_000_000_000  # 20GB
max_concurrent_requests = 4
max_context_tokens = 1_000_000
max_queued_tasks = 1000
max_checkpoint_size = 2_000_000_000  # 2GB

[llm]
provider = "vllm"
model_path = "/data/models/qwen3-coder-32b"
tensor_parallel_size = 1
gpu_memory_utilization = 0.85
max_model_len = 1_000_000
max_num_seqs = 256
enable_prefix_caching = true
enable_chunked_prefill = true

[llm.quantization]
auto_adjust = true
preferred = "none"
fallback = ["fp8", "int8", "int4"]

[llm.context]
max_tokens = 1_000_000
compression_strategy = "Hierarchical"
summary_interval = 10

[llm.queue]
max_concurrent = 4
enable_preemption = true
preemption_mode = "swap"  # or "recompute"

[observability.logging]
level = "info"
format = "json"
log_dir = "/data/logs"
retention_days = 7
max_file_size_mb = 100

[observability.metrics]
enabled = true
prometheus_port = 9090
export_interval_seconds = 15

[observability.tracing]
enabled = true
jaeger_endpoint = "http://localhost:4317"
sampling_rate = 0.1

[intervention]
enabled = true
port = 8080
default_timeout_seconds = 1800

[intervention.policy]
default_on_timeout = "Pause"

[intervention.policy.auto_approve]
patterns = [
    { type = "FileModification", path_pattern = "^/tmp/" },
    { type = "CodeExecution", language = "rust" },
]

[intervention.policy.require_approval]
patterns = [
    { type = "SelfModification" },
    { type = "SystemCommand" },
    { type = "NetworkRequest", url_pattern = ".*external.*" },
]

[intervention.notifications]
webhook_url = ""
slack_webhook = ""
email = ""
```

---

## 11. Operational Runbooks

### 11.1 Starting a Multi-Day Run

```bash
# Start with explicit session ID
export SELFWARE_SESSION_ID="improvement-run-$(date +%Y%m%d)"
docker-compose up -d

# Monitor startup
docker-compose logs -f selfware | grep -E "(started|error|checkpoint)"

# Watch initial checkpoint creation
curl http://localhost:8080/health
```

### 11.2 Monitoring During Run

```bash
# Health status
curl http://localhost:8080/health | jq

# Current tasks
curl http://localhost:8080/interventions | jq

# Prometheus metrics
curl http://localhost:9090/metrics | grep selfware

# Live logs
docker-compose logs -f selfware --tail=100
```

### 11.3 Recovery Procedures

```bash
# Check for crash recovery
docker-compose logs selfware | grep -i "recovery"

# Manual recovery from specific checkpoint
docker-compose run --rm selfware selfware recover --checkpoint-id=<id>

# Force restart with recovery
docker-compose down
docker-compose up -d -e SELFWARE_RECOVER=true
```

### 11.4 Graceful Shutdown

```bash
# Trigger graceful shutdown (creates final checkpoint)
docker-compose stop -t 120 selfware

# Verify checkpoint
docker-compose run --rm selfware ls -la /data/checkpoints/
```

---

## 12. Summary

This infrastructure design provides:

1. **Comprehensive Checkpointing**: Multi-level checkpointing with incremental storage, content-defined chunking, and automatic recovery
2. **Robust Process Supervision**: Hierarchical supervision trees, circuit breakers, health checks, and watchdog timers
3. **Intelligent Resource Management**: GPU monitoring, memory pressure handling, disk management, and adaptive quotas
4. **Optimized LLM Inference**: Model lifecycle management, vLLM integration, context compression, and request prioritization
5. **Full Observability**: Structured logging, Prometheus metrics, distributed tracing, and health endpoints
6. **Human Oversight**: Intervention API, approval workflows, progress dashboard, and configurable policies
7. **Production-Ready Deployment**: Docker containers with health checks, Kubernetes manifests, and operational runbooks

The design supports 3-7+ days of autonomous operation with graceful degradation, automatic recovery, and human intervention points for critical decisions.
