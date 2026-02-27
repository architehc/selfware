# Selfware RSI: Multi-Day Execution Infrastructure

Infrastructure for 3-7 day autonomous operation with Qwen3 Coder local inference, crash recovery, and resource management.

---

## 1. Checkpointing System

**Purpose:** Persist execution state at regular intervals to enable crash recovery without losing progress. Captures agent state, working memory, and pending task queue.

### Key Design Decisions

- **Dual checkpoint strategy:** Time-based (every 5 minutes) + event-based (after task completion)
- **Differential checkpoints:** Full snapshot every 30min, delta updates between
- **Storage:** SQLite for metadata + bincode for state blobs
- **Retention:** Keep last 10 checkpoints, archive daily snapshots
- **Async I/O:** Checkpoint in background thread to avoid blocking

### What to Checkpoint

| Component | Frequency | Format | Size Target |
|-----------|-----------|--------|-------------|
| Agent State | 5min + events | bincode | <10MB |
| Task Queue | Every change | SQLite | - |
| Working Memory | 5min | bincode + compression | <50MB |
| LLM Context | On demand | safetensors | <100MB |
| Metrics/Logs | Continuous | SQLite + parquet | - |

### Rust Implementation

```rust
// Crates: serde, bincode, rusqlite, tokio, zstd

use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use std::time::{Duration, Instant};

/// Checkpoint manager with dual strategy
pub struct CheckpointManager {
    db: rusqlite::Connection,
    state_dir: PathBuf,
    last_full: Instant,
    interval_full: Duration,
    interval_delta: Duration,
}

#[derive(Serialize, Deserialize)]
pub struct AgentCheckpoint {
    pub version: u64,
    pub timestamp: u64,
    pub state: AgentState,
    pub memory: WorkingMemory,
    pub queue: TaskQueue,
    pub llm_context: Option<ContextSnapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct AgentState {
    pub current_task: Option<TaskId>,
    pub iteration_count: u64,
    pub session_start: u64,
    pub config: AgentConfig,
}

impl CheckpointManager {
    /// Create checkpoint (full or delta based on timing)
    pub async fn checkpoint(&mut self, agent: &Agent) -> Result<CheckpointId> {
        let is_full = self.last_full.elapsed() >= self.interval_full;
        if is_full {
            self.create_full_checkpoint(agent).await
        } else {
            self.create_delta_checkpoint(agent).await
        }
    }
    
    /// Restore from latest valid checkpoint
    pub async fn restore(&self) -> Result<Option<AgentCheckpoint>> {
        // Query SQLite for latest checkpoint metadata
        // Verify checksum, decompress, deserialize
    }
}
```

---

## 2. Process Supervision

**Purpose:** Detect agent crashes and unhealthy states, automatically restart with state recovery. Provides heartbeat monitoring and graceful shutdown handling.

### Key Design Decisions

- **Supervisor pattern:** Separate watchdog process monitors agent
- **Heartbeat:** Agent pings every 30s, supervisor kills after 90s silence
- **Restart policy:** Exponential backoff (1s, 2s, 4s, 8s, max 60s)
- **Health checks:** HTTP endpoint + custom health probes
- **Graceful shutdown:** SIGTERM handling with 30s timeout

### Failure Detection Matrix

| Failure Type | Detection | Action | Max Restarts |
|--------------|-----------|--------|--------------|
| Process crash | PID monitoring | Restart + restore | 5/hour |
| Heartbeat timeout | 90s no ping | Kill + restart | 3/hour |
| Memory leak | RSS > threshold | Restart + alert | 2/hour |
| GPU hang | CUDA timeout | Restart vLLM | 3/hour |
| Disk full | <5% free | Pause + alert | Manual |

### Rust Implementation

```rust
// Crates: tokio, nix (signals), sysinfo, anyhow

use tokio::time::{interval, timeout};
use tokio::sync::mpsc;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

/// Process supervisor with restart policy
pub struct Supervisor {
    config: SupervisorConfig,
    state: SupervisorState,
    restart_history: Vec<Instant>,
    health_tx: mpsc::Sender<HealthStatus>,
}

pub struct SupervisorConfig {
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
    pub max_restarts_per_hour: u32,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    pub graceful_shutdown_timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

impl Supervisor {
    /// Main supervision loop
    pub async fn run(&mut self) -> Result<()> {
        loop {
            let mut child = self.spawn_agent().await?;
            
            tokio::select! {
                // Process exited
                status = child.wait() => {
                    log::error!("Agent exited: {:?}", status);
                }
                // Heartbeat timeout
                _ = self.wait_heartbeat() => {
                    log::error!("Heartbeat timeout, killing agent");
                    child.kill().await?;
                }
                // Shutdown signal
                _ = self.shutdown_signal() => {
                    self.graceful_shutdown(child).await?;
                    return Ok(());
                }
            }
            
            self.handle_restart().await?;
        }
    }
    
    /// Calculate backoff delay based on recent restarts
    fn calculate_backoff(&self) -> Duration {
        let recent = self.restart_history
            .iter()
            .filter(|t| t.elapsed() < Duration::from_secs(3600))
            .count();
        
        let delay = self.config.backoff_base * 2_u32.pow(recent as u32);
        delay.min(self.config.backoff_max)
    }
}

/// Agent-side heartbeat sender
pub struct HeartbeatClient {
    interval: Duration,
    supervisor_addr: String,
}

impl HeartbeatClient {
    pub async fn run(&self) {
        let mut interval = interval(self.interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.send_heartbeat().await {
                log::warn!("Failed to send heartbeat: {}", e);
            }
        }
    }
}
```

---

## 3. Resource Management

**Purpose:** Monitor and control GPU VRAM, system RAM, and disk usage. Automatically adjust model quantization and handle memory pressure.

### Key Design Decisions

- **Continuous monitoring:** 5-second polling for all resources
- **VRAM tiers:** Normal (<70%), Warning (70-85%), Critical (>85%)
- **Quantization fallback:** Q4 → Q3 → Q2 on memory pressure
- **Model swapping:** Unload model to disk when idle >10min
- **Memory pressure:** Trigger GC at 80% RAM, OOM killer protection

### Resource Thresholds

| Resource | Normal | Warning | Critical | Action |
|----------|--------|---------|----------|--------|
| GPU VRAM | <70% | 70-85% | >85% | Quantize down |
| System RAM | <70% | 70-85% | >85% | Force GC, pause |
| Disk | >20% | 10-20% | <10% | Alert, pause |
| CPU | <80% | 80-95% | >95% | Throttle tasks |

### Model Quantization Strategy

```
Normal operation:    Qwen3-32B-Q4_K_M (18GB VRAM)
Memory pressure:     Qwen3-32B-Q3_K_L (14GB VRAM)
Critical memory:     Qwen3-32B-Q2_K   (10GB VRAM)
Idle timeout:        Unload model, reload on demand
```

### Rust Implementation

```rust
// Crates: nvml-wrapper, sysinfo, tokio, llm

use nvml_wrapper::NVML;
use sysinfo::{System, SystemExt, ProcessExt};
use tokio::sync::watch;

/// Resource monitor with automatic adjustment
pub struct ResourceManager {
    nvml: NVML,
    system: System,
    gpu_thresholds: GpuThresholds,
    ram_thresholds: RamThresholds,
    current_tier: ResourceTier,
    model_manager: ModelManager,
    status_tx: watch::Sender<ResourceStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResourceTier {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct ResourceStatus {
    pub gpu_vram_used_percent: f32,
    pub ram_used_percent: f32,
    pub disk_free_gb: u64,
    pub tier: ResourceTier,
    pub model_quantization: QuantizationLevel,
}

#[derive(Debug, Clone, Copy)]
pub enum QuantizationLevel {
    Q4_K_M,  // 4-bit, best quality
    Q3_K_L,  // 3-bit, good quality
    Q2_K,    // 2-bit, acceptable
}

pub struct ModelManager {
    current_model: Option<ModelHandle>,
    quantization: QuantizationLevel,
    last_used: Instant,
    idle_timeout: Duration,
    vllm_client: VllmClient,
}

impl ResourceManager {
    /// Main monitoring loop
    pub async fn monitor_loop(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            
            let status = self.collect_metrics().await;
            self.status_tx.send(status.clone()).ok();
            
            match status.tier {
                ResourceTier::Normal => self.maintain_current_model(),
                ResourceTier::Warning => self.reduce_quantization().await,
                ResourceTier::Critical => self.emergency_actions().await,
            }
            
            self.handle_idle_model(&status).await;
        }
    }
    
    /// Reduce model precision under memory pressure
    async fn reduce_quantization(&mut self) {
        let current = self.model_manager.quantization;
        let next = match current {
            QuantizationLevel::Q4_K_M => QuantizationLevel::Q3_K_L,
            QuantizationLevel::Q3_K_L => QuantizationLevel::Q2_K,
            QuantizationLevel::Q2_K => return, // Already at minimum
        };
        
        log::warn!("Reducing quantization: {:?} -> {:?}", current, next);
        self.model_manager.switch_quantization(next).await;
    }
    
    /// Emergency: unload model, force GC, pause processing
    async fn emergency_actions(&mut self) {
        log::error!("Critical resource pressure! Taking emergency actions");
        
        self.model_manager.unload_model().await;
        
        // Force Rust allocator to return memory to OS
        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
        
        // Signal agent to pause task processing
        self.pause_agent().await;
    }
    
    /// Collect current resource metrics
    async fn collect_metrics(&mut self) -> ResourceStatus {
        self.system.refresh_all();
        
        let gpu = self.nvml.device_by_index(0).unwrap();
        let mem_info = gpu.memory_info().unwrap();
        let vram_percent = (mem_info.used as f32 / mem_info.total as f32) * 100.0;
        
        let ram_percent = self.system.used_memory() as f32 / 
                         self.system.total_memory() as f32 * 100.0;
        
        let tier = if vram_percent > 85.0 || ram_percent > 85.0 {
            ResourceTier::Critical
        } else if vram_percent > 70.0 || ram_percent > 70.0 {
            ResourceTier::Warning
        } else {
            ResourceTier::Normal
        };
        
        ResourceStatus {
            gpu_vram_used_percent: vram_percent,
            ram_used_percent: ram_percent,
            disk_free_gb: self.get_disk_free(),
            tier,
            model_quantization: self.model_manager.quantization,
        }
    }
}

/// vLLM client for model management
pub struct VllmClient {
    base_url: String,
    http: reqwest::Client,
}

impl VllmClient {
    /// Load model with specific quantization
    pub async fn load_model(&self, model: &str, quant: QuantizationLevel) -> Result<()> {
        let quant_str = match quant {
            QuantizationLevel::Q4_K_M => "Q4_K_M",
            QuantizationLevel::Q3_K_L => "Q3_K_L",
            QuantizationLevel::Q2_K => "Q2_K",
        };
        
        self.http.post(format!("{}/v1/load", self.base_url))
            .json(&json!({
                "model": model,
                "quantization": quant_str,
                "gpu_memory_utilization": 0.85,
            }))
            .send()
            .await?;
        
        Ok(())
    }
    
    /// Unload model to free VRAM
    pub async fn unload_model(&self) -> Result<()> {
        self.http.post(format!("{}/v1/unload", self.base_url))
            .send()
            .await?;
        Ok(())
    }
}
```

---

## 4. Progress Tracking

**Purpose:** Track execution metrics, task completion rates, and system health. Enable human visibility into multi-day runs via dashboard and alerts.

### Key Design Decisions

- **Metrics:** Task throughput, success rate, iteration count, resource usage
- **Storage:** Time-series in SQLite (recent) + parquet (archive)
- **Dashboard:** Web UI with real-time updates via WebSocket
- **Alerts:** Webhook notifications on anomalies
- **Human checkpoint:** Daily summary report with key metrics

### Metrics to Track

| Category | Metric | Aggregation | Retention |
|----------|--------|-------------|-----------|
| Tasks | Completed/hour | Hourly average | 30 days |
| Tasks | Success rate | Rolling 24h | 30 days |
| Iterations | Total count | Cumulative | Forever |
| LLM | Tokens/sec | Hourly average | 7 days |
| LLM | Latency p99 | Rolling 1h | 7 days |
| Resources | VRAM/RAM usage | 5min samples | 7 days |
| Errors | Count by type | Hourly | 30 days |
| Recovery | Restart count | Daily | 30 days |

### Rust Implementation

```rust
// Crates: metrics, metrics-exporter-prometheus, rusqlite, parquet

use metrics::{counter, gauge, histogram};
use std::collections::VecDeque;
use chrono::{DateTime, Utc};

/// Progress tracker with time-series storage
pub struct ProgressTracker {
    db: rusqlite::Connection,
    metrics_buffer: VecDeque<MetricPoint>,
    buffer_size: usize,
    alert_config: AlertConfig,
}

#[derive(Debug, Clone)]
pub struct MetricPoint {
    pub timestamp: DateTime<Utc>,
    pub metric_type: MetricType,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum MetricType {
    TaskCompleted,
    TaskFailed,
    IterationCompleted,
    LlmTokensGenerated,
    LlmLatencyMs,
    GpuVramUsed,
    RamUsed,
}

#[derive(Debug, Clone)]
pub struct ProgressSummary {
    pub session_start: DateTime<Utc>,
    pub total_iterations: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub success_rate: f64,
    pub avg_tokens_per_sec: f64,
    pub current_vram_usage: f64,
    pub last_checkpoint: DateTime<Utc>,
    pub uptime_hours: f64,
}

impl ProgressTracker {
    /// Record a metric point
    pub fn record(&mut self, metric_type: MetricType, value: f64) {
        let point = MetricPoint {
            timestamp: Utc::now(),
            metric_type,
            value,
            labels: HashMap::new(),
        };
        
        self.metrics_buffer.push_back(point);
        
        if self.metrics_buffer.len() >= self.buffer_size {
            self.flush_buffer();
        }
    }
    
    /// Generate daily summary report
    pub fn generate_daily_summary(&self) -> Result<ProgressSummary> {
        let row = self.db.query_row(
            "SELECT 
                COUNT(*) as iterations,
                SUM(CASE WHEN task_status = 'completed' THEN 1 ELSE 0 END) as completed,
                SUM(CASE WHEN task_status = 'failed' THEN 1 ELSE 0 END) as failed,
                AVG(llm_tokens_per_sec) as avg_tps
             FROM metrics 
             WHERE timestamp > datetime('now', '-1 day')",
            [],
            |row| {
                Ok(ProgressSummary {
                    session_start: self.get_session_start(),
                    total_iterations: row.get(0)?,
                    tasks_completed: row.get(1)?,
                    tasks_failed: row.get(2)?,
                    success_rate: 0.0, // calculated below
                    avg_tokens_per_sec: row.get(3)?,
                    current_vram_usage: self.get_current_vram(),
                    last_checkpoint: self.get_last_checkpoint(),
                    uptime_hours: self.get_uptime(),
                })
            }
        )?;
        
        Ok(row)
    }
    
    /// Check for anomalies and send alerts
    pub async fn check_alerts(&self) {
        // Success rate drop
        if self.get_success_rate_1h() < 0.5 {
            self.send_alert(AlertType::LowSuccessRate).await;
        }
        
        // No progress
        if self.get_iterations_last_hour() == 0 {
            self.send_alert(AlertType::NoProgress).await;
        }
        
        // High error rate
        if self.get_error_rate_1h() > 0.1 {
            self.send_alert(AlertType::HighErrorRate).await;
        }
    }
}

/// Web dashboard server
pub struct DashboardServer {
    progress: Arc<RwLock<ProgressTracker>>,
    clients: Arc<RwLock<Vec<WebSocketSender>>>,
}

impl DashboardServer {
    pub async fn run(&self, addr: &str) -> Result<()> {
        let app = Router::new()
            .route("/api/summary", get(get_summary))
            .route("/api/metrics", get(get_metrics))
            .route("/ws", get(websocket_handler))
            .layer(Extension(self.progress.clone()));
        
        axum::Server::bind(&addr.parse()?)
            .serve(app.into_make_service())
            .await?;
        
        Ok(())
    }
    
    /// Broadcast real-time updates to connected clients
    pub async fn broadcast_update(&self, update: ProgressUpdate) {
        let clients = self.clients.read().await;
        for client in clients.iter() {
            let _ = client.send(serde_json::to_string(&update).unwrap()).await;
        }
    }
}
```

---

## 5. Recovery Workflow

**Purpose:** Detect crashes, restore state from last checkpoint, and continue execution seamlessly. Handles partial failures and data consistency.

### Key Design Decisions

- **Crash detection:** Supervisor heartbeat timeout + exit code analysis
- **State validation:** Checksum verification before restoration
- **Idempotent tasks:** Tasks designed to be safely re-executed
- **Partial recovery:** Resume from last completed task if checkpoint corrupt
- **Human intervention:** Escalation after 5 failed recovery attempts

### Recovery Flow

```
1. DETECT: Supervisor detects agent crash/timeout
   ↓
2. VALIDATE: Check last checkpoint integrity (checksum + version)
   ↓
3. RESTORE: Load state, memory, queue from checkpoint
   ↓
4. RECONCILE: Verify task queue consistency with execution log
   ↓
5. RESUME: Continue from last known good state
   ↓
6. VERIFY: Confirm successful recovery, alert if issues
```

### Recovery Scenarios

| Scenario | Detection | Recovery Action | Fallback |
|----------|-----------|-----------------|----------|
| Clean checkpoint | Checksum OK | Full restore | - |
| Corrupt checkpoint | Checksum fail | Use previous checkpoint | Manual |
| No checkpoint | File missing | Fresh start with config | - |
| Partial task | Log mismatch | Re-execute last task | Skip + log |
| LLM unavailable | Connection fail | Retry with backoff | Pause |

### Rust Implementation

```rust
// Crates: thiserror, crc32fast, serde_json

use thiserror::Error;
use crc32fast::Hasher;

/// Recovery orchestrator
pub struct RecoveryManager {
    checkpoint_mgr: CheckpointManager,
    task_log: TaskLog,
    max_retries: u32,
    retry_count: u32,
}

#[derive(Error, Debug)]
pub enum RecoveryError {
    #[error("No valid checkpoint found")]
    NoCheckpoint,
    #[error("Checkpoint corrupt: {0}")]
    CorruptCheckpoint(String),
    #[error("State version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u64, actual: u64 },
    #[error("Max recovery attempts exceeded")]
    MaxRetriesExceeded,
    #[error("Task reconciliation failed: {0}")]
    ReconciliationFailed(String),
}

#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub success: bool,
    pub checkpoint_id: CheckpointId,
    pub restored_state: AgentState,
    pub tasks_reconciled: usize,
    pub tasks_reexecuted: usize,
    pub recovery_time_ms: u64,
}

impl RecoveryManager {
    /// Main recovery entry point
    pub async fn recover(&mut self) -> Result<RecoveryResult, RecoveryError> {
        let start = Instant::now();
        
        // 1. Find and validate checkpoint
        let checkpoint = self.find_valid_checkpoint().await?;
        
        // 2. Restore state
        let mut state = self.restore_state(&checkpoint).await?;
        
        // 3. Reconcile task queue with execution log
        let (reconciled, reexecuted) = self.reconcile_tasks(&mut state).await?;
        
        // 4. Verify recovery
        self.verify_recovery(&state).await?;
        
        self.retry_count = 0;
        
        Ok(RecoveryResult {
            success: true,
            checkpoint_id: checkpoint.id,
            restored_state: state,
            tasks_reconciled: reconciled,
            tasks_reexecuted: reexecuted,
            recovery_time_ms: start.elapsed().as_millis() as u64,
        })
    }
    
    /// Find most recent valid checkpoint
    async fn find_valid_checkpoint(&self) -> Result<AgentCheckpoint, RecoveryError> {
        let checkpoints = self.checkpoint_mgr.list_checkpoints().await?;
        
        for cp in checkpoints.iter().rev() {
            if self.validate_checkpoint(cp).await? {
                return Ok(cp.clone());
            }
            log::warn!("Checkpoint {} failed validation, trying older", cp.id);
        }
        
        Err(RecoveryError::NoCheckpoint)
    }
    
    /// Validate checkpoint integrity
    async fn validate_checkpoint(&self, cp: &AgentCheckpoint) -> Result<bool, RecoveryError> {
        // Verify version compatibility
        if cp.version > CURRENT_STATE_VERSION {
            return Err(RecoveryError::VersionMismatch {
                expected: CURRENT_STATE_VERSION,
                actual: cp.version,
            });
        }
        
        // Verify checksum
        let data = bincode::serialize(&cp.state)
            .map_err(|e| RecoveryError::CorruptCheckpoint(e.to_string()))?;
        
        let mut hasher = Hasher::new();
        hasher.update(&data);
        let checksum = hasher.finalize();
        
        // Compare with stored checksum
        Ok(checksum == cp.state_checksum)
    }
    
    /// Reconcile task queue with execution log
    async fn reconcile_tasks(&self, state: &mut AgentState) -> Result<(usize, usize), RecoveryError> {
        let mut reconciled = 0;
        let mut reexecuted = 0;
        
        for task in state.queue.pending.iter_mut() {
            match self.task_log.get_status(&task.id).await? {
                TaskStatus::Completed => {
                    // Task completed after checkpoint, mark done
                    task.status = TaskStatus::Completed;
                    reconciled += 1;
                }
                TaskStatus::Failed => {
                    // Task failed, retry if attempts < max
                    if task.attempts < task.max_attempts {
                        task.status = TaskStatus::Pending;
                        task.attempts += 1;
                        reexecuted += 1;
                    }
                }
                TaskStatus::InProgress => {
                    // Task was in progress during crash, re-execute
                    task.status = TaskStatus::Pending;
                    reexecuted += 1;
                }
                TaskStatus::Pending => {
                    // No change needed
                }
            }
        }
        
        Ok((reconciled, reexecuted))
    }
    
    /// Handle recovery failure with escalation
    pub async fn handle_recovery_failure(&mut self, error: RecoveryError) -> Result<(), RecoveryError> {
        self.retry_count += 1;
        
        log::error!("Recovery failed (attempt {}): {}", self.retry_count, error);
        
        if self.retry_count >= self.max_retries {
            // Escalate to human intervention
            self.send_escalation_alert(&error).await;
            return Err(RecoveryError::MaxRetriesExceeded);
        }
        
        // Wait before retry
        let backoff = Duration::from_secs(2_u64.pow(self.retry_count));
        tokio::time::sleep(backoff).await;
        
        // Try recovery again
        self.recover().await
    }
}

/// Task log for idempotency tracking
pub struct TaskLog {
    db: rusqlite::Connection,
}

impl TaskLog {
    /// Record task execution outcome
    pub async fn record(&self, task_id: &TaskId, status: TaskStatus) -> Result<()> {
        self.db.execute(
            "INSERT INTO task_log (task_id, status, timestamp) 
             VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id) DO UPDATE SET
             status = excluded.status,
             timestamp = excluded.timestamp",
            (task_id.to_string(), status.to_string(), Utc::now().timestamp()),
        )?;
        Ok(())
    }
    
    /// Get task execution status
    pub async fn get_status(&self, task_id: &TaskId) -> Result<TaskStatus> {
        let status: String = self.db.query_row(
            "SELECT status FROM task_log WHERE task_id = ?1",
            [task_id.to_string()],
            |row| row.get(0),
        )?;
        
        Ok(TaskStatus::from_str(&status)?)
    }
}
```

---

## Integration: Main Agent Loop

```rust
/// Main agent with all infrastructure integrated
pub struct ResilientAgent {
    checkpoint_mgr: CheckpointManager,
    supervisor: Supervisor,
    resource_mgr: ResourceManager,
    progress: ProgressTracker,
    recovery: RecoveryManager,
    state: AgentState,
}

impl ResilientAgent {
    pub async fn run(&mut self) -> Result<()> {
        // Attempt recovery if resuming
        if let Some(checkpoint) = self.recovery.attempt_recovery().await? {
            self.state = checkpoint.state;
            log::info!("Resumed from checkpoint {}", checkpoint.id);
        }
        
        // Start resource monitoring
        let resource_handle = tokio::spawn(async move {
            self.resource_mgr.monitor_loop().await;
        });
        
        // Start progress tracking
        let progress_handle = tokio::spawn(async move {
            self.progress.reporting_loop().await;
        });
        
        // Main execution loop
        loop {
            // Checkpoint before each iteration
            self.checkpoint_mgr.checkpoint(self).await?;
            
            // Execute one iteration
            match self.execute_iteration().await {
                Ok(()) => {
                    self.progress.record(MetricType::IterationCompleted, 1.0);
                }
                Err(e) => {
                    log::error!("Iteration failed: {}", e);
                    self.progress.record(MetricType::TaskFailed, 1.0);
                    
                    // Short pause before retry
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}
```

---

## Crate Dependencies

```toml
[dependencies]
# Core async
tokio = { version = "1.35", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
serde_json = "1.0"

# Database
rusqlite = { version = "0.30", features = ["bundled", "chrono"] }

# Compression
zstd = "0.13"

# GPU monitoring
nvml-wrapper = "0.10"

# System monitoring
sysinfo = "0.30"

# HTTP/Web
axum = "0.7"
tokio-tungstenite = "0.21"
reqwest = { version = "0.11", features = ["json"] }

# Metrics
metrics = "0.22"
metrics-exporter-prometheus = "0.13"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Signals
nix = { version = "0.27", features = ["signal"] }

# Checksums
crc32fast = "1.3"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

---

## Summary

| Component | Key Crates | Critical Feature |
|-----------|------------|------------------|
| Checkpointing | bincode, rusqlite, zstd | Differential + compression |
| Supervision | tokio, nix | Exponential backoff |
| Resource Mgmt | nvml-wrapper, sysinfo | Auto-quantization fallback |
| Progress | metrics, axum | Real-time dashboard |
| Recovery | thiserror, crc32fast | Idempotent task log |
