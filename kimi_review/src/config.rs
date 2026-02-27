//! Configuration management for Selfware

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// System-wide configuration
    pub system: SystemConfig,
    /// Checkpoint configuration
    pub checkpoint: CheckpointConfig,
    /// Supervision configuration
    pub supervision: SupervisionConfig,
    /// Resource management configuration
    pub resources: ResourcesConfig,
    /// LLM inference configuration
    pub llm: LLMConfig,
    /// Observability configuration
    pub observability: ObservabilityConfig,
    /// Intervention configuration
    pub intervention: InterventionConfig,
}

impl Config {
    /// Load configuration from files and environment
    pub fn load() -> crate::Result<Self> {
        let config_dir = std::env::var("SELFWARE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./config"));
        
        let mut builder = config::Config::builder()
            .add_source(config::File::from(config_dir.join("default.toml")).required(false))
            .add_source(config::File::from(config_dir.join("production.toml")).required(false))
            .add_source(config::Environment::with_prefix("SELFWARE").separator("_"));
        
        // Add local config if it exists
        let local_config = config_dir.join("local.toml");
        if local_config.exists() {
            builder = builder.add_source(config::File::from(local_config).required(false));
        }
        
        let config = builder.build()
            .map_err(|e| crate::error::SelfwareError::Config(e.to_string()))?;
        
        config.try_deserialize()
            .map_err(|e| crate::error::SelfwareError::Config(e.to_string()).into())
    }
    
    /// Load from a specific file path
    pub fn from_file(path: impl Into<PathBuf>) -> crate::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::from(path.into()).required(true))
            .add_source(config::Environment::with_prefix("SELFWARE").separator("_"))
            .build()
            .map_err(|e| crate::error::SelfwareError::Config(e.to_string()))?;
        
        config.try_deserialize()
            .map_err(|e| crate::error::SelfwareError::Config(e.to_string()).into())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            system: SystemConfig::default(),
            checkpoint: CheckpointConfig::default(),
            supervision: SupervisionConfig::default(),
            resources: ResourcesConfig::default(),
            llm: LLMConfig::default(),
            observability: ObservabilityConfig::default(),
            intervention: InterventionConfig::default(),
        }
    }
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Session ID (auto-generated if not set)
    pub session_id: String,
    /// Maximum runtime in hours
    pub max_runtime_hours: u32,
    /// Grace period for shutdown in seconds
    pub shutdown_grace_period_seconds: u32,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            session_id: "auto".to_string(),
            max_runtime_hours: 168, // 7 days
            shutdown_grace_period_seconds: 60,
        }
    }
}

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Enable checkpointing
    pub enabled: bool,
    /// Checkpoint interval in seconds
    pub interval_seconds: u64,
    /// Compression algorithm
    pub compression: CompressionAlgorithm,
    /// Compression level (0-9)
    pub compression_level: u32,
    /// Storage path
    pub storage_path: PathBuf,
    /// Maximum checkpoint size in bytes
    pub max_size_bytes: u64,
    /// Retention period in days
    pub retention_days: u32,
    /// Enable incremental checkpoints
    pub incremental: bool,
    /// Enable content-defined chunking
    pub content_defined_chunking: bool,
    /// Checkpoint level configurations
    pub levels: CheckpointLevels,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionAlgorithm {
    None,
    Zstd,
    Gzip,
    Lz4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointLevels {
    pub micro: LevelConfig,
    pub task: LevelConfig,
    pub session: LevelConfig,
    pub system: LevelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelConfig {
    pub enabled: bool,
    pub interval_seconds: Option<u64>,
    pub on_completion: Option<bool>,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 300, // 5 minutes
            compression: CompressionAlgorithm::Zstd,
            compression_level: 6,
            storage_path: PathBuf::from("./checkpoints"),
            max_size_bytes: 10_737_418_240, // 10GB
            retention_days: 7,
            incremental: true,
            content_defined_chunking: true,
            levels: CheckpointLevels {
                micro: LevelConfig {
                    enabled: true,
                    interval_seconds: Some(30),
                    on_completion: None,
                },
                task: LevelConfig {
                    enabled: true,
                    interval_seconds: None,
                    on_completion: Some(true),
                },
                session: LevelConfig {
                    enabled: true,
                    interval_seconds: Some(300),
                    on_completion: None,
                },
                system: LevelConfig {
                    enabled: true,
                    interval_seconds: Some(900),
                    on_completion: None,
                },
            },
        }
    }
}

/// Supervision configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisionConfig {
    /// Supervision strategy
    pub strategy: SupervisionStrategy,
    /// Maximum restarts in time window
    pub max_restarts: u32,
    /// Time window for max restarts
    pub max_seconds: u32,
    /// Backoff strategy
    pub backoff_strategy: BackoffStrategy,
    /// Health check configuration
    pub health_check: HealthCheckConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackoffStrategy {
    Fixed { seconds: u64 },
    Linear { base_seconds: u64, multiplier: f32 },
    Exponential { base_seconds: u64, max_seconds: u64 },
}

impl BackoffStrategy {
    /// Get duration for retry attempt
    pub fn duration(&self, attempt: u32) -> Duration {
        match self {
            Self::Fixed { seconds } => Duration::from_secs(*seconds),
            Self::Linear { base_seconds, multiplier } => {
                Duration::from_secs((*base_seconds as f32 * (1.0 + *multiplier * attempt as f32)) as u64)
            }
            Self::Exponential { base_seconds, max_seconds } => {
                let exp = (2_u64).pow(attempt.min(10)); // Cap at 2^10
                let secs = (*base_seconds * exp).min(*max_seconds);
                Duration::from_secs(secs)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub failure_threshold: u32,
}

impl Default for SupervisionConfig {
    fn default() -> Self {
        Self {
            strategy: SupervisionStrategy::OneForOne,
            max_restarts: 5,
            max_seconds: 60,
            backoff_strategy: BackoffStrategy::Exponential {
                base_seconds: 1,
                max_seconds: 60,
            },
            health_check: HealthCheckConfig {
                interval_seconds: 10,
                timeout_seconds: 5,
                failure_threshold: 3,
            },
        }
    }
}

/// Resource management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesConfig {
    pub gpu: GpuConfig,
    pub memory: MemoryConfig,
    pub disk: DiskConfig,
    pub quotas: ResourceQuotas,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    pub monitor_interval_seconds: u64,
    pub temperature_threshold: u32,
    pub memory_utilization_threshold: f32,
    pub throttle_on_overheat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub warning_threshold: f32,
    pub critical_threshold: f32,
    pub emergency_threshold: f32,
    pub monitor_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskConfig {
    pub max_usage_percent: f32,
    pub maintenance_interval_seconds: u64,
    pub compress_after_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuotas {
    pub max_gpu_memory_per_model: u64,
    pub max_concurrent_requests: usize,
    pub max_context_tokens: usize,
    pub max_queued_tasks: usize,
    pub max_checkpoint_size: u64,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            gpu: GpuConfig {
                monitor_interval_seconds: 5,
                temperature_threshold: 85,
                memory_utilization_threshold: 0.95,
                throttle_on_overheat: true,
            },
            memory: MemoryConfig {
                warning_threshold: 0.70,
                critical_threshold: 0.85,
                emergency_threshold: 0.95,
                monitor_interval_seconds: 2,
            },
            disk: DiskConfig {
                max_usage_percent: 0.85,
                maintenance_interval_seconds: 3600,
                compress_after_days: 1,
            },
            quotas: ResourceQuotas {
                max_gpu_memory_per_model: 20_000_000_000, // 20GB
                max_concurrent_requests: 4,
                max_context_tokens: 1_000_000,
                max_queued_tasks: 1000,
                max_checkpoint_size: 2_000_000_000, // 2GB
            },
        }
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    /// Provider (vllm, ollama)
    pub provider: String,
    /// Model path
    pub model_path: PathBuf,
    /// Tensor parallel size
    pub tensor_parallel_size: usize,
    /// GPU memory utilization (0.0-1.0)
    pub gpu_memory_utilization: f32,
    /// Maximum model context length
    pub max_model_len: usize,
    /// Maximum concurrent sequences
    pub max_num_seqs: usize,
    /// Enable prefix caching
    pub enable_prefix_caching: bool,
    /// Enable chunked prefill
    pub enable_chunked_prefill: bool,
    /// Quantization configuration
    pub quantization: QuantizationConfig,
    /// Context management
    pub context: ContextConfig,
    /// Request queue configuration
    pub queue: QueueConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    pub auto_adjust: bool,
    pub preferred: String,
    pub fallback: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub max_tokens: usize,
    pub compression_strategy: String,
    pub summary_interval: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub max_concurrent: usize,
    pub enable_preemption: bool,
    pub preemption_mode: String,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            provider: "vllm".to_string(),
            model_path: PathBuf::from("/data/models/qwen3-coder"),
            tensor_parallel_size: 1,
            gpu_memory_utilization: 0.85,
            max_model_len: 1_000_000,
            max_num_seqs: 256,
            enable_prefix_caching: true,
            enable_chunked_prefill: true,
            quantization: QuantizationConfig {
                auto_adjust: true,
                preferred: "none".to_string(),
                fallback: vec!["fp8".to_string(), "int8".to_string(), "int4".to_string()],
            },
            context: ContextConfig {
                max_tokens: 1_000_000,
                compression_strategy: "Hierarchical".to_string(),
                summary_interval: 10,
            },
            queue: QueueConfig {
                max_concurrent: 4,
                enable_preemption: true,
                preemption_mode: "swap".to_string(),
            },
        }
    }
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub logging: LoggingConfig,
    pub metrics: MetricsConfig,
    pub tracing: TracingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub log_dir: PathBuf,
    pub retention_days: u32,
    pub max_file_size_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub prometheus_port: u16,
    pub export_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    pub jaeger_endpoint: String,
    pub sampling_rate: f64,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                log_dir: PathBuf::from("./logs"),
                retention_days: 7,
                max_file_size_mb: 100,
            },
            metrics: MetricsConfig {
                enabled: true,
                prometheus_port: 9090,
                export_interval_seconds: 15,
            },
            tracing: TracingConfig {
                enabled: true,
                jaeger_endpoint: "http://localhost:4317".to_string(),
                sampling_rate: 0.1,
            },
        }
    }
}

/// Intervention configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionConfig {
    pub enabled: bool,
    pub port: u16,
    pub default_timeout_seconds: u64,
    pub policy: InterventionPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionPolicyConfig {
    pub default_on_timeout: String,
}

impl Default for InterventionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8080,
            default_timeout_seconds: 1800,
            policy: InterventionPolicyConfig {
                default_on_timeout: "pause".to_string(),
            },
        }
    }
}
