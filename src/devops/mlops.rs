//! MLOps & AI Development Tools
//!
//! Provides experiment tracking, model versioning, training pipeline debugging,
//! and feature store management for ML/AI development workflows.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static EXPERIMENT_COUNTER: AtomicU64 = AtomicU64::new(1);
static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);
static MODEL_COUNTER: AtomicU64 = AtomicU64::new(1);
static FEATURE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Experiment Tracking
// ============================================================================

/// Experiment tracking platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackingPlatform {
    /// MLflow tracking
    MLflow,
    /// Weights & Biases
    WandB,
    /// TensorBoard
    TensorBoard,
    /// Custom tracking
    Custom,
}

/// Metric value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    /// Metric name
    pub name: String,
    /// Metric value
    pub value: f64,
    /// Step/iteration number
    pub step: u64,
    /// Timestamp
    pub timestamp: u64,
}

impl MetricPoint {
    pub fn new(name: impl Into<String>, value: f64, step: u64) -> Self {
        Self {
            name: name.into(),
            value,
            step,
            timestamp: current_timestamp(),
        }
    }
}

/// Parameter for experiment run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunParameter {
    /// Parameter name
    pub name: String,
    /// Parameter value (as string)
    pub value: String,
    /// Parameter type hint
    pub param_type: ParameterType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterType {
    String,
    Integer,
    Float,
    Boolean,
    Json,
}

impl RunParameter {
    pub fn string(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            param_type: ParameterType::String,
        }
    }

    pub fn integer(name: impl Into<String>, value: i64) -> Self {
        Self {
            name: name.into(),
            value: value.to_string(),
            param_type: ParameterType::Integer,
        }
    }

    pub fn float(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value: value.to_string(),
            param_type: ParameterType::Float,
        }
    }

    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value: value.to_string(),
            param_type: ParameterType::Boolean,
        }
    }
}

/// Artifact from a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunArtifact {
    /// Artifact name
    pub name: String,
    /// Artifact path
    pub path: String,
    /// Artifact type
    pub artifact_type: ArtifactType,
    /// Size in bytes
    pub size_bytes: u64,
    /// Checksum
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactType {
    Model,
    Dataset,
    Checkpoint,
    Config,
    Log,
    Plot,
    Other,
}

/// Status of an experiment run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Killed,
    Scheduled,
}

/// Single experiment run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRun {
    /// Unique run ID
    pub run_id: String,
    /// Run name
    pub name: String,
    /// Parent experiment ID
    pub experiment_id: String,
    /// Run status
    pub status: RunStatus,
    /// Start time
    pub start_time: u64,
    /// End time
    pub end_time: Option<u64>,
    /// Run parameters
    pub parameters: Vec<RunParameter>,
    /// Logged metrics
    pub metrics: Vec<MetricPoint>,
    /// Run artifacts
    pub artifacts: Vec<RunArtifact>,
    /// Tags
    pub tags: HashMap<String, String>,
    /// Git commit hash
    pub git_commit: Option<String>,
    /// Source file
    pub source_file: Option<String>,
}

impl ExperimentRun {
    pub fn new(name: impl Into<String>, experiment_id: impl Into<String>) -> Self {
        let run_id = format!("run_{}", RUN_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            run_id,
            name: name.into(),
            experiment_id: experiment_id.into(),
            status: RunStatus::Running,
            start_time: current_timestamp(),
            end_time: None,
            parameters: Vec::new(),
            metrics: Vec::new(),
            artifacts: Vec::new(),
            tags: HashMap::new(),
            git_commit: None,
            source_file: None,
        }
    }

    pub fn log_param(&mut self, param: RunParameter) {
        self.parameters.push(param);
    }

    pub fn log_metric(&mut self, name: &str, value: f64, step: u64) {
        self.metrics.push(MetricPoint::new(name, value, step));
    }

    pub fn log_artifact(&mut self, artifact: RunArtifact) {
        self.artifacts.push(artifact);
    }

    pub fn set_tag(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.tags.insert(key.into(), value.into());
    }

    pub fn finish(&mut self, status: RunStatus) {
        self.status = status;
        self.end_time = Some(current_timestamp());
    }

    pub fn duration(&self) -> Option<Duration> {
        self.end_time
            .map(|end| Duration::from_secs(end - self.start_time))
    }

    pub fn best_metric(&self, name: &str) -> Option<f64> {
        self.metrics
            .iter()
            .filter(|m| m.name == name)
            .map(|m| m.value)
            .fold(None, |acc, v| match acc {
                None => Some(v),
                Some(best) => Some(if v > best { v } else { best }),
            })
    }
}

/// Experiment containing multiple runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Unique experiment ID
    pub experiment_id: String,
    /// Experiment name
    pub name: String,
    /// Description
    pub description: String,
    /// Tracking platform
    pub platform: TrackingPlatform,
    /// Runs in this experiment
    pub runs: Vec<ExperimentRun>,
    /// Created timestamp
    pub created_at: u64,
    /// Tags
    pub tags: HashMap<String, String>,
}

impl Experiment {
    pub fn new(name: impl Into<String>, platform: TrackingPlatform) -> Self {
        let experiment_id = format!("exp_{}", EXPERIMENT_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            experiment_id,
            name: name.into(),
            description: String::new(),
            platform,
            runs: Vec::new(),
            created_at: current_timestamp(),
            tags: HashMap::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn start_run(&mut self, name: impl Into<String>) -> &mut ExperimentRun {
        let run = ExperimentRun::new(name, &self.experiment_id);
        self.runs.push(run);
        self.runs.last_mut().unwrap()
    }

    pub fn get_run(&self, run_id: &str) -> Option<&ExperimentRun> {
        self.runs.iter().find(|r| r.run_id == run_id)
    }

    pub fn get_run_mut(&mut self, run_id: &str) -> Option<&mut ExperimentRun> {
        self.runs.iter_mut().find(|r| r.run_id == run_id)
    }

    pub fn best_run(&self, metric: &str) -> Option<&ExperimentRun> {
        self.runs
            .iter()
            .filter(|r| r.status == RunStatus::Completed)
            .max_by(|a, b| {
                let a_val = a.best_metric(metric).unwrap_or(f64::NEG_INFINITY);
                let b_val = b.best_metric(metric).unwrap_or(f64::NEG_INFINITY);
                a_val.partial_cmp(&b_val).unwrap()
            })
    }

    pub fn completed_runs(&self) -> Vec<&ExperimentRun> {
        self.runs
            .iter()
            .filter(|r| r.status == RunStatus::Completed)
            .collect()
    }
}

/// Experiment tracker for managing experiments and runs
#[derive(Debug, Clone)]
pub struct ExperimentTracker {
    /// All experiments
    pub experiments: HashMap<String, Experiment>,
    /// Default tracking platform
    pub default_platform: TrackingPlatform,
}

impl ExperimentTracker {
    pub fn new(default_platform: TrackingPlatform) -> Self {
        Self {
            experiments: HashMap::new(),
            default_platform,
        }
    }

    pub fn create_experiment(&mut self, name: impl Into<String>) -> &mut Experiment {
        let experiment = Experiment::new(name, self.default_platform);
        let id = experiment.experiment_id.clone();
        self.experiments.insert(id.clone(), experiment);
        self.experiments.get_mut(&id).unwrap()
    }

    pub fn get_experiment(&self, id: &str) -> Option<&Experiment> {
        self.experiments.get(id)
    }

    pub fn get_experiment_mut(&mut self, id: &str) -> Option<&mut Experiment> {
        self.experiments.get_mut(id)
    }

    pub fn list_experiments(&self) -> Vec<&Experiment> {
        self.experiments.values().collect()
    }

    pub fn compare_runs(&self, run_ids: &[&str], metric: &str) -> Vec<(String, Option<f64>)> {
        run_ids
            .iter()
            .map(|run_id| {
                let value = self
                    .experiments
                    .values()
                    .flat_map(|e| e.runs.iter())
                    .find(|r| r.run_id == *run_id)
                    .and_then(|r| r.best_metric(metric));
                (run_id.to_string(), value)
            })
            .collect()
    }
}

// ============================================================================
// Model Versioning & Lineage
// ============================================================================

/// Model stage in lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStage {
    None,
    Staging,
    Production,
    Archived,
}

/// Model version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    /// Version number
    pub version: u32,
    /// Model stage
    pub stage: ModelStage,
    /// Source run ID
    pub run_id: Option<String>,
    /// Model artifact path
    pub artifact_path: String,
    /// Framework (pytorch, tensorflow, etc.)
    pub framework: String,
    /// Created timestamp
    pub created_at: u64,
    /// Description
    pub description: String,
    /// Metrics at time of registration
    pub metrics: HashMap<String, f64>,
    /// Model signature (input/output schema)
    pub signature: Option<ModelSignature>,
}

/// Model input/output signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSignature {
    /// Input schema
    pub inputs: Vec<TensorSpec>,
    /// Output schema
    pub outputs: Vec<TensorSpec>,
}

/// Tensor specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorSpec {
    /// Tensor name
    pub name: String,
    /// Data type
    pub dtype: String,
    /// Shape (None for dynamic dimensions)
    pub shape: Vec<Option<i64>>,
}

impl TensorSpec {
    pub fn new(name: impl Into<String>, dtype: impl Into<String>, shape: Vec<Option<i64>>) -> Self {
        Self {
            name: name.into(),
            dtype: dtype.into(),
            shape,
        }
    }
}

/// Registered model with versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredModel {
    /// Model ID
    pub model_id: String,
    /// Model name
    pub name: String,
    /// Description
    pub description: String,
    /// Model versions
    pub versions: Vec<ModelVersion>,
    /// Tags
    pub tags: HashMap<String, String>,
    /// Created timestamp
    pub created_at: u64,
    /// Last updated timestamp
    pub updated_at: u64,
}

impl RegisteredModel {
    pub fn new(name: impl Into<String>) -> Self {
        let model_id = format!("model_{}", MODEL_COUNTER.fetch_add(1, Ordering::SeqCst));
        let now = current_timestamp();
        Self {
            model_id,
            name: name.into(),
            description: String::new(),
            versions: Vec::new(),
            tags: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn register_version(
        &mut self,
        artifact_path: impl Into<String>,
        framework: impl Into<String>,
    ) -> &mut ModelVersion {
        let version = ModelVersion {
            version: self.versions.len() as u32 + 1,
            stage: ModelStage::None,
            run_id: None,
            artifact_path: artifact_path.into(),
            framework: framework.into(),
            created_at: current_timestamp(),
            description: String::new(),
            metrics: HashMap::new(),
            signature: None,
        };
        self.versions.push(version);
        self.updated_at = current_timestamp();
        self.versions.last_mut().unwrap()
    }

    pub fn get_version(&self, version: u32) -> Option<&ModelVersion> {
        self.versions.iter().find(|v| v.version == version)
    }

    pub fn get_version_mut(&mut self, version: u32) -> Option<&mut ModelVersion> {
        self.versions.iter_mut().find(|v| v.version == version)
    }

    pub fn production_version(&self) -> Option<&ModelVersion> {
        self.versions
            .iter()
            .find(|v| v.stage == ModelStage::Production)
    }

    pub fn staging_version(&self) -> Option<&ModelVersion> {
        self.versions
            .iter()
            .find(|v| v.stage == ModelStage::Staging)
    }

    pub fn transition_stage(&mut self, version: u32, stage: ModelStage) -> Result<(), String> {
        // If transitioning to production, move current production to archived
        if stage == ModelStage::Production {
            for v in &mut self.versions {
                if v.stage == ModelStage::Production {
                    v.stage = ModelStage::Archived;
                }
            }
        }

        let version_obj = self
            .get_version_mut(version)
            .ok_or_else(|| format!("Version {} not found", version))?;
        version_obj.stage = stage;
        self.updated_at = current_timestamp();
        Ok(())
    }

    pub fn latest_version(&self) -> Option<&ModelVersion> {
        self.versions.last()
    }
}

/// Model lineage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLineage {
    /// Model ID
    pub model_id: String,
    /// Parent model (if fine-tuned from another)
    pub parent_model: Option<String>,
    /// Training dataset IDs
    pub training_datasets: Vec<String>,
    /// Feature set used
    pub feature_set: Option<String>,
    /// Training run ID
    pub training_run: Option<String>,
    /// Preprocessing pipeline
    pub preprocessing: Vec<String>,
    /// Downstream models (models derived from this)
    pub downstream_models: Vec<String>,
}

impl ModelLineage {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            parent_model: None,
            training_datasets: Vec::new(),
            feature_set: None,
            training_run: None,
            preprocessing: Vec::new(),
            downstream_models: Vec::new(),
        }
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_model = Some(parent.into());
        self
    }

    pub fn add_dataset(&mut self, dataset_id: impl Into<String>) {
        self.training_datasets.push(dataset_id.into());
    }
}

/// Model registry for managing registered models
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    /// Registered models
    pub models: HashMap<String, RegisteredModel>,
    /// Model lineage
    pub lineage: HashMap<String, ModelLineage>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            lineage: HashMap::new(),
        }
    }

    pub fn register_model(&mut self, name: impl Into<String>) -> &mut RegisteredModel {
        let model = RegisteredModel::new(name);
        let id = model.model_id.clone();
        self.models.insert(id.clone(), model);
        self.lineage.insert(id.clone(), ModelLineage::new(&id));
        self.models.get_mut(&id).unwrap()
    }

    pub fn get_model(&self, id: &str) -> Option<&RegisteredModel> {
        self.models.get(id)
    }

    pub fn get_model_by_name(&self, name: &str) -> Option<&RegisteredModel> {
        self.models.values().find(|m| m.name == name)
    }

    pub fn get_lineage(&self, model_id: &str) -> Option<&ModelLineage> {
        self.lineage.get(model_id)
    }

    pub fn list_production_models(&self) -> Vec<(&RegisteredModel, &ModelVersion)> {
        self.models
            .values()
            .filter_map(|m| m.production_version().map(|v| (m, v)))
            .collect()
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Training Pipeline Debugging
// ============================================================================

/// Training step type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrainingStepType {
    DataLoading,
    Preprocessing,
    Forward,
    Loss,
    Backward,
    OptimizerStep,
    Validation,
    Checkpoint,
    Logging,
}

/// Training step timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTiming {
    /// Step type
    pub step_type: TrainingStepType,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Epoch number
    pub epoch: u32,
    /// Batch number
    pub batch: u32,
}

/// Gradient statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientStats {
    /// Layer name
    pub layer: String,
    /// Mean gradient value
    pub mean: f64,
    /// Standard deviation
    pub std: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Norm
    pub norm: f64,
    /// Has NaN values
    pub has_nan: bool,
    /// Has Inf values
    pub has_inf: bool,
}

impl GradientStats {
    pub fn is_healthy(&self) -> bool {
        !self.has_nan && !self.has_inf && self.norm < 1000.0 && self.norm > 1e-10
    }

    pub fn diagnosis(&self) -> Option<String> {
        if self.has_nan {
            Some("NaN gradients detected - check for division by zero or overflow".to_string())
        } else if self.has_inf {
            Some("Inf gradients detected - learning rate may be too high".to_string())
        } else if self.norm > 1000.0 {
            Some("Exploding gradients - consider gradient clipping".to_string())
        } else if self.norm < 1e-10 {
            Some("Vanishing gradients - check activation functions and initialization".to_string())
        } else {
            None
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Total allocated memory (bytes)
    pub allocated: u64,
    /// Peak allocated memory (bytes)
    pub peak_allocated: u64,
    /// Reserved memory (bytes)
    pub reserved: u64,
    /// Free memory in reserved (bytes)
    pub free: u64,
    /// Active allocations count
    pub num_allocations: u32,
}

impl MemoryStats {
    pub fn utilization(&self) -> f64 {
        if self.reserved == 0 {
            0.0
        } else {
            self.allocated as f64 / self.reserved as f64
        }
    }

    pub fn fragmentation(&self) -> f64 {
        if self.reserved == 0 {
            0.0
        } else {
            self.free as f64 / self.reserved as f64
        }
    }
}

/// Training anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingAnomaly {
    /// Anomaly type
    pub anomaly_type: AnomalyType,
    /// Description
    pub description: String,
    /// Epoch where detected
    pub epoch: u32,
    /// Batch where detected
    pub batch: Option<u32>,
    /// Severity
    pub severity: AnomalySeverity,
    /// Suggested fix
    pub suggestion: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyType {
    GradientExploding,
    GradientVanishing,
    LossSpike,
    LossPlateau,
    LearningRateIssue,
    MemoryLeak,
    SlowDataLoading,
    NumericalInstability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Training debugger
#[derive(Debug, Clone)]
pub struct TrainingDebugger {
    /// Step timings
    pub timings: Vec<StepTiming>,
    /// Gradient statistics history
    pub gradient_history: Vec<Vec<GradientStats>>,
    /// Memory statistics history
    pub memory_history: Vec<MemoryStats>,
    /// Loss history
    pub loss_history: Vec<(u32, f64)>,
    /// Learning rate history
    pub lr_history: Vec<(u32, f64)>,
    /// Detected anomalies
    pub anomalies: Vec<TrainingAnomaly>,
}

impl TrainingDebugger {
    pub fn new() -> Self {
        Self {
            timings: Vec::new(),
            gradient_history: Vec::new(),
            memory_history: Vec::new(),
            loss_history: Vec::new(),
            lr_history: Vec::new(),
            anomalies: Vec::new(),
        }
    }

    pub fn record_timing(
        &mut self,
        step_type: TrainingStepType,
        duration_ms: u64,
        epoch: u32,
        batch: u32,
    ) {
        self.timings.push(StepTiming {
            step_type,
            duration_ms,
            epoch,
            batch,
        });
    }

    pub fn record_gradients(&mut self, stats: Vec<GradientStats>) {
        // Check for issues
        for stat in &stats {
            if let Some(diagnosis) = stat.diagnosis() {
                self.anomalies.push(TrainingAnomaly {
                    anomaly_type: if stat.has_nan || stat.has_inf {
                        AnomalyType::NumericalInstability
                    } else if stat.norm > 1000.0 {
                        AnomalyType::GradientExploding
                    } else {
                        AnomalyType::GradientVanishing
                    },
                    description: format!("Layer {}: {}", stat.layer, diagnosis),
                    epoch: 0, // Would need current epoch
                    batch: None,
                    severity: if stat.has_nan || stat.has_inf {
                        AnomalySeverity::Critical
                    } else {
                        AnomalySeverity::High
                    },
                    suggestion: diagnosis,
                });
            }
        }
        self.gradient_history.push(stats);
    }

    pub fn record_memory(&mut self, stats: MemoryStats) {
        // Check for memory issues
        if let Some(prev) = self.memory_history.last() {
            if stats.allocated > prev.allocated * 2 {
                self.anomalies.push(TrainingAnomaly {
                    anomaly_type: AnomalyType::MemoryLeak,
                    description: "Memory usage doubled between steps".to_string(),
                    epoch: 0,
                    batch: None,
                    severity: AnomalySeverity::High,
                    suggestion: "Check for accumulating tensors or disable gradient tracking where not needed".to_string(),
                });
            }
        }
        self.memory_history.push(stats);
    }

    pub fn record_loss(&mut self, epoch: u32, loss: f64) {
        // Check for loss issues
        if let Some((_, prev_loss)) = self.loss_history.last() {
            if loss > prev_loss * 10.0 {
                self.anomalies.push(TrainingAnomaly {
                    anomaly_type: AnomalyType::LossSpike,
                    description: format!("Loss spiked from {} to {}", prev_loss, loss),
                    epoch,
                    batch: None,
                    severity: AnomalySeverity::High,
                    suggestion: "Consider reducing learning rate or checking for bad data"
                        .to_string(),
                });
            }
        }
        self.loss_history.push((epoch, loss));
    }

    pub fn record_learning_rate(&mut self, epoch: u32, lr: f64) {
        self.lr_history.push((epoch, lr));
    }

    pub fn bottleneck_analysis(&self) -> HashMap<TrainingStepType, f64> {
        let mut totals: HashMap<TrainingStepType, u64> = HashMap::new();
        let mut counts: HashMap<TrainingStepType, u64> = HashMap::new();

        for timing in &self.timings {
            *totals.entry(timing.step_type).or_default() += timing.duration_ms;
            *counts.entry(timing.step_type).or_default() += 1;
        }

        totals
            .into_iter()
            .map(|(step_type, total)| {
                let count = counts.get(&step_type).unwrap_or(&1);
                (step_type, total as f64 / *count as f64)
            })
            .collect()
    }

    pub fn is_data_loading_bottleneck(&self) -> bool {
        let analysis = self.bottleneck_analysis();
        let data_time = analysis.get(&TrainingStepType::DataLoading).unwrap_or(&0.0);
        let forward_time = analysis.get(&TrainingStepType::Forward).unwrap_or(&0.0);
        data_time > forward_time
    }

    pub fn loss_trend(&self) -> Option<f64> {
        if self.loss_history.len() < 2 {
            return None;
        }

        let n = self.loss_history.len() as f64;
        let sum_x: f64 = (0..self.loss_history.len()).map(|i| i as f64).sum();
        let sum_y: f64 = self.loss_history.iter().map(|(_, l)| l).sum();
        let sum_xy: f64 = self
            .loss_history
            .iter()
            .enumerate()
            .map(|(i, (_, l))| i as f64 * l)
            .sum();
        let sum_x2: f64 = (0..self.loss_history.len()).map(|i| (i * i) as f64).sum();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
        Some(slope)
    }

    pub fn critical_anomalies(&self) -> Vec<&TrainingAnomaly> {
        self.anomalies
            .iter()
            .filter(|a| a.severity == AnomalySeverity::Critical)
            .collect()
    }
}

impl Default for TrainingDebugger {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Feature Store Management
// ============================================================================

/// Feature data type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureDataType {
    Integer,
    Float,
    String,
    Boolean,
    Vector,
    Timestamp,
    Categorical,
}

/// Feature definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDefinition {
    /// Feature ID
    pub feature_id: String,
    /// Feature name
    pub name: String,
    /// Description
    pub description: String,
    /// Data type
    pub dtype: FeatureDataType,
    /// Default value (as JSON string)
    pub default_value: Option<String>,
    /// Transformation applied
    pub transformation: Option<String>,
    /// Source (table, API, etc.)
    pub source: String,
    /// Entity the feature belongs to
    pub entity: String,
    /// Tags
    pub tags: Vec<String>,
    /// Created timestamp
    pub created_at: u64,
    /// Created by
    pub created_by: String,
}

impl FeatureDefinition {
    pub fn new(
        name: impl Into<String>,
        dtype: FeatureDataType,
        source: impl Into<String>,
        entity: impl Into<String>,
    ) -> Self {
        let feature_id = format!("feat_{}", FEATURE_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            feature_id,
            name: name.into(),
            description: String::new(),
            dtype,
            default_value: None,
            transformation: None,
            source: source.into(),
            entity: entity.into(),
            tags: Vec::new(),
            created_at: current_timestamp(),
            created_by: String::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_transformation(mut self, transformation: impl Into<String>) -> Self {
        self.transformation = Some(transformation.into());
        self
    }
}

/// Feature set (group of features)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSet {
    /// Feature set ID
    pub set_id: String,
    /// Name
    pub name: String,
    /// Description
    pub description: String,
    /// Feature IDs in this set
    pub feature_ids: Vec<String>,
    /// Entity type
    pub entity: String,
    /// Version
    pub version: u32,
    /// Created timestamp
    pub created_at: u64,
}

impl FeatureSet {
    pub fn new(name: impl Into<String>, entity: impl Into<String>) -> Self {
        Self {
            set_id: format!("fset_{}", FEATURE_COUNTER.fetch_add(1, Ordering::SeqCst)),
            name: name.into(),
            description: String::new(),
            feature_ids: Vec::new(),
            entity: entity.into(),
            version: 1,
            created_at: current_timestamp(),
        }
    }

    pub fn add_feature(&mut self, feature_id: impl Into<String>) {
        self.feature_ids.push(feature_id.into());
    }
}

/// Feature statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStats {
    /// Feature ID
    pub feature_id: String,
    /// Count of non-null values
    pub count: u64,
    /// Null count
    pub null_count: u64,
    /// Unique values (for categorical)
    pub unique_count: Option<u64>,
    /// Mean (for numeric)
    pub mean: Option<f64>,
    /// Standard deviation (for numeric)
    pub std: Option<f64>,
    /// Minimum (for numeric)
    pub min: Option<f64>,
    /// Maximum (for numeric)
    pub max: Option<f64>,
    /// Histogram (bucket -> count)
    pub histogram: Option<Vec<(String, u64)>>,
    /// Computed timestamp
    pub computed_at: u64,
}

impl FeatureStats {
    pub fn new(feature_id: impl Into<String>) -> Self {
        Self {
            feature_id: feature_id.into(),
            count: 0,
            null_count: 0,
            unique_count: None,
            mean: None,
            std: None,
            min: None,
            max: None,
            histogram: None,
            computed_at: current_timestamp(),
        }
    }

    pub fn null_rate(&self) -> f64 {
        let total = self.count + self.null_count;
        if total == 0 {
            0.0
        } else {
            self.null_count as f64 / total as f64
        }
    }

    pub fn cardinality(&self) -> Option<f64> {
        self.unique_count
            .map(|u| u as f64 / self.count.max(1) as f64)
    }
}

/// Feature freshness check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFreshness {
    /// Feature ID
    pub feature_id: String,
    /// Last update timestamp
    pub last_updated: u64,
    /// Expected update frequency (seconds)
    pub expected_frequency: u64,
    /// Is stale
    pub is_stale: bool,
    /// Staleness duration (seconds)
    pub staleness_seconds: u64,
}

impl FeatureFreshness {
    pub fn check(
        feature_id: impl Into<String>,
        last_updated: u64,
        expected_frequency: u64,
    ) -> Self {
        let now = current_timestamp();
        let age = now.saturating_sub(last_updated);
        let is_stale = age > expected_frequency;
        Self {
            feature_id: feature_id.into(),
            last_updated,
            expected_frequency,
            is_stale,
            staleness_seconds: if is_stale {
                age - expected_frequency
            } else {
                0
            },
        }
    }
}

/// Feature store
#[derive(Debug, Clone)]
pub struct FeatureStore {
    /// Feature definitions
    pub features: HashMap<String, FeatureDefinition>,
    /// Feature sets
    pub feature_sets: HashMap<String, FeatureSet>,
    /// Feature statistics
    pub stats: HashMap<String, FeatureStats>,
    /// Feature freshness
    pub freshness: HashMap<String, FeatureFreshness>,
}

impl FeatureStore {
    pub fn new() -> Self {
        Self {
            features: HashMap::new(),
            feature_sets: HashMap::new(),
            stats: HashMap::new(),
            freshness: HashMap::new(),
        }
    }

    pub fn register_feature(&mut self, feature: FeatureDefinition) -> &FeatureDefinition {
        let id = feature.feature_id.clone();
        self.features.insert(id.clone(), feature);
        self.features.get(&id).unwrap()
    }

    pub fn create_feature_set(
        &mut self,
        name: impl Into<String>,
        entity: impl Into<String>,
    ) -> &mut FeatureSet {
        let set = FeatureSet::new(name, entity);
        let id = set.set_id.clone();
        self.feature_sets.insert(id.clone(), set);
        self.feature_sets.get_mut(&id).unwrap()
    }

    pub fn get_feature(&self, id: &str) -> Option<&FeatureDefinition> {
        self.features.get(id)
    }

    pub fn get_feature_set(&self, id: &str) -> Option<&FeatureSet> {
        self.feature_sets.get(id)
    }

    pub fn update_stats(&mut self, feature_id: &str, stats: FeatureStats) {
        self.stats.insert(feature_id.to_string(), stats);
    }

    pub fn check_freshness(
        &mut self,
        feature_id: &str,
        last_updated: u64,
        expected_frequency: u64,
    ) -> &FeatureFreshness {
        let freshness = FeatureFreshness::check(feature_id, last_updated, expected_frequency);
        self.freshness.insert(feature_id.to_string(), freshness);
        self.freshness.get(feature_id).unwrap()
    }

    pub fn stale_features(&self) -> Vec<&FeatureFreshness> {
        self.freshness.values().filter(|f| f.is_stale).collect()
    }

    pub fn features_by_entity(&self, entity: &str) -> Vec<&FeatureDefinition> {
        self.features
            .values()
            .filter(|f| f.entity == entity)
            .collect()
    }

    pub fn search_features(&self, query: &str) -> Vec<&FeatureDefinition> {
        let query_lower = query.to_lowercase();
        self.features
            .values()
            .filter(|f| {
                f.name.to_lowercase().contains(&query_lower)
                    || f.description.to_lowercase().contains(&query_lower)
                    || f.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    pub fn data_quality_report(&self) -> Vec<(String, f64, Option<String>)> {
        self.stats
            .iter()
            .map(|(id, stats)| {
                let null_rate = stats.null_rate();
                let issue = if null_rate > 0.5 {
                    Some("High null rate".to_string())
                } else if stats.cardinality().map(|c| c > 0.95).unwrap_or(false) {
                    Some("Very high cardinality".to_string())
                } else {
                    None
                };
                (id.clone(), 1.0 - null_rate, issue)
            })
            .collect()
    }
}

impl Default for FeatureStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Experiment tracking tests
    #[test]
    fn test_metric_point() {
        let metric = MetricPoint::new("accuracy", 0.95, 100);
        assert_eq!(metric.name, "accuracy");
        assert_eq!(metric.value, 0.95);
        assert_eq!(metric.step, 100);
    }

    #[test]
    fn test_run_parameter_types() {
        let string_param = RunParameter::string("model", "bert");
        assert_eq!(string_param.param_type, ParameterType::String);

        let int_param = RunParameter::integer("epochs", 10);
        assert_eq!(int_param.value, "10");

        let float_param = RunParameter::float("lr", 0.001);
        assert_eq!(float_param.param_type, ParameterType::Float);

        let bool_param = RunParameter::boolean("debug", true);
        assert_eq!(bool_param.value, "true");
    }

    #[test]
    fn test_experiment_run_creation() {
        let run = ExperimentRun::new("training_run_1", "exp_1");
        assert!(run.run_id.starts_with("run_"));
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.end_time.is_none());
    }

    #[test]
    fn test_experiment_run_logging() {
        let mut run = ExperimentRun::new("test_run", "exp_1");
        run.log_param(RunParameter::float("learning_rate", 0.001));
        run.log_metric("loss", 1.5, 0);
        run.log_metric("loss", 1.2, 1);
        run.log_metric("loss", 0.9, 2);
        run.set_tag("env", "development");

        assert_eq!(run.parameters.len(), 1);
        assert_eq!(run.metrics.len(), 3);
        assert_eq!(run.tags.get("env"), Some(&"development".to_string()));
    }

    #[test]
    fn test_experiment_run_best_metric() {
        let mut run = ExperimentRun::new("test", "exp");
        run.log_metric("accuracy", 0.8, 0);
        run.log_metric("accuracy", 0.85, 1);
        run.log_metric("accuracy", 0.9, 2);

        assert_eq!(run.best_metric("accuracy"), Some(0.9));
        assert_eq!(run.best_metric("unknown"), None);
    }

    #[test]
    fn test_experiment_run_finish() {
        let mut run = ExperimentRun::new("test", "exp");
        assert!(run.duration().is_none());

        run.finish(RunStatus::Completed);
        assert_eq!(run.status, RunStatus::Completed);
        assert!(run.end_time.is_some());
    }

    #[test]
    fn test_experiment_creation() {
        let exp = Experiment::new("my_experiment", TrackingPlatform::MLflow)
            .with_description("Test experiment");
        assert!(exp.experiment_id.starts_with("exp_"));
        assert_eq!(exp.name, "my_experiment");
        assert_eq!(exp.platform, TrackingPlatform::MLflow);
    }

    #[test]
    fn test_experiment_runs() {
        let mut exp = Experiment::new("test", TrackingPlatform::WandB);
        let run = exp.start_run("run_1");
        run.log_metric("acc", 0.9, 0);
        run.finish(RunStatus::Completed);

        assert_eq!(exp.runs.len(), 1);
        assert_eq!(exp.completed_runs().len(), 1);
    }

    #[test]
    fn test_experiment_best_run() {
        let mut exp = Experiment::new("test", TrackingPlatform::Custom);

        let run1 = exp.start_run("run_1");
        run1.log_metric("accuracy", 0.8, 0);
        run1.finish(RunStatus::Completed);

        let run2 = exp.start_run("run_2");
        run2.log_metric("accuracy", 0.95, 0);
        run2.finish(RunStatus::Completed);

        let best = exp.best_run("accuracy").unwrap();
        assert_eq!(best.best_metric("accuracy"), Some(0.95));
    }

    #[test]
    fn test_experiment_tracker() {
        let mut tracker = ExperimentTracker::new(TrackingPlatform::MLflow);
        let exp = tracker.create_experiment("test_exp");
        let run = exp.start_run("run_1");
        run.log_metric("f1", 0.85, 0);

        assert_eq!(tracker.list_experiments().len(), 1);
    }

    // Model versioning tests
    #[test]
    fn test_tensor_spec() {
        let spec = TensorSpec::new(
            "input",
            "float32",
            vec![None, Some(224), Some(224), Some(3)],
        );
        assert_eq!(spec.name, "input");
        assert_eq!(spec.shape.len(), 4);
    }

    #[test]
    fn test_registered_model() {
        let mut model = RegisteredModel::new("bert-classifier")
            .with_description("BERT for text classification");

        assert!(model.model_id.starts_with("model_"));
        assert_eq!(model.name, "bert-classifier");
        assert!(model.versions.is_empty());

        let version = model.register_version("/models/bert/v1", "pytorch");
        assert_eq!(version.version, 1);
        assert_eq!(version.stage, ModelStage::None);
    }

    #[test]
    fn test_model_stage_transition() {
        let mut model = RegisteredModel::new("test_model");
        model.register_version("/v1", "tensorflow");
        model.register_version("/v2", "tensorflow");

        // Transition v1 to production
        model.transition_stage(1, ModelStage::Production).unwrap();
        assert_eq!(model.get_version(1).unwrap().stage, ModelStage::Production);

        // Transition v2 to production, v1 should be archived
        model.transition_stage(2, ModelStage::Production).unwrap();
        assert_eq!(model.get_version(1).unwrap().stage, ModelStage::Archived);
        assert_eq!(model.get_version(2).unwrap().stage, ModelStage::Production);
    }

    #[test]
    fn test_model_registry() {
        let mut registry = ModelRegistry::new();
        let model = registry.register_model("my_model");
        model.register_version("/path", "pytorch");
        let model_id = model.model_id.clone();

        assert!(registry.get_model_by_name("my_model").is_some());
        assert!(registry.get_lineage(&model_id).is_some());
    }

    #[test]
    fn test_model_lineage() {
        let lineage = ModelLineage::new("model_1").with_parent("model_0");
        assert_eq!(lineage.parent_model, Some("model_0".to_string()));
    }

    // Training debugger tests
    #[test]
    fn test_gradient_stats_healthy() {
        let stats = GradientStats {
            layer: "layer1".to_string(),
            mean: 0.01,
            std: 0.1,
            min: -0.5,
            max: 0.5,
            norm: 1.0,
            has_nan: false,
            has_inf: false,
        };
        assert!(stats.is_healthy());
        assert!(stats.diagnosis().is_none());
    }

    #[test]
    fn test_gradient_stats_exploding() {
        let stats = GradientStats {
            layer: "layer1".to_string(),
            mean: 100.0,
            std: 500.0,
            min: -1000.0,
            max: 1000.0,
            norm: 2000.0,
            has_nan: false,
            has_inf: false,
        };
        assert!(!stats.is_healthy());
        assert!(stats.diagnosis().unwrap().contains("Exploding"));
    }

    #[test]
    fn test_gradient_stats_vanishing() {
        let stats = GradientStats {
            layer: "layer1".to_string(),
            mean: 1e-15,
            std: 1e-15,
            min: 0.0,
            max: 1e-14,
            norm: 1e-12,
            has_nan: false,
            has_inf: false,
        };
        assert!(!stats.is_healthy());
        assert!(stats.diagnosis().unwrap().contains("Vanishing"));
    }

    #[test]
    fn test_gradient_stats_nan() {
        let stats = GradientStats {
            layer: "layer1".to_string(),
            mean: f64::NAN,
            std: 0.0,
            min: 0.0,
            max: 0.0,
            norm: 1.0,
            has_nan: true,
            has_inf: false,
        };
        assert!(!stats.is_healthy());
        assert!(stats.diagnosis().unwrap().contains("NaN"));
    }

    #[test]
    fn test_memory_stats() {
        let stats = MemoryStats {
            allocated: 1_000_000,
            peak_allocated: 1_500_000,
            reserved: 2_000_000,
            free: 1_000_000,
            num_allocations: 100,
        };
        assert_eq!(stats.utilization(), 0.5);
        assert_eq!(stats.fragmentation(), 0.5);
    }

    #[test]
    fn test_training_debugger_timing() {
        let mut debugger = TrainingDebugger::new();
        debugger.record_timing(TrainingStepType::DataLoading, 100, 0, 0);
        debugger.record_timing(TrainingStepType::Forward, 50, 0, 0);
        debugger.record_timing(TrainingStepType::Backward, 60, 0, 0);

        let analysis = debugger.bottleneck_analysis();
        assert!(analysis.contains_key(&TrainingStepType::DataLoading));
    }

    #[test]
    fn test_training_debugger_data_bottleneck() {
        let mut debugger = TrainingDebugger::new();
        debugger.record_timing(TrainingStepType::DataLoading, 200, 0, 0);
        debugger.record_timing(TrainingStepType::Forward, 50, 0, 0);

        assert!(debugger.is_data_loading_bottleneck());
    }

    #[test]
    fn test_training_debugger_loss_spike() {
        let mut debugger = TrainingDebugger::new();
        debugger.record_loss(0, 1.0);
        debugger.record_loss(1, 0.8);
        debugger.record_loss(2, 15.0); // Spike!

        assert!(!debugger.anomalies.is_empty());
        assert_eq!(debugger.anomalies[0].anomaly_type, AnomalyType::LossSpike);
    }

    #[test]
    fn test_training_debugger_loss_trend() {
        let mut debugger = TrainingDebugger::new();
        debugger.record_loss(0, 2.0);
        debugger.record_loss(1, 1.5);
        debugger.record_loss(2, 1.0);
        debugger.record_loss(3, 0.5);

        let trend = debugger.loss_trend().unwrap();
        assert!(trend < 0.0); // Decreasing loss
    }

    // Feature store tests
    #[test]
    fn test_feature_definition() {
        let feature =
            FeatureDefinition::new("user_age", FeatureDataType::Integer, "users_table", "user")
                .with_description("User's age in years")
                .with_transformation("CAST(birthdate AS age)");

        assert!(feature.feature_id.starts_with("feat_"));
        assert_eq!(feature.name, "user_age");
        assert!(feature.transformation.is_some());
    }

    #[test]
    fn test_feature_set() {
        let mut set = FeatureSet::new("user_features", "user");
        set.add_feature("feat_1");
        set.add_feature("feat_2");

        assert_eq!(set.feature_ids.len(), 2);
        assert_eq!(set.entity, "user");
    }

    #[test]
    fn test_feature_stats() {
        let mut stats = FeatureStats::new("feat_1");
        stats.count = 1000;
        stats.null_count = 50;
        stats.unique_count = Some(100);

        assert_eq!(stats.null_rate(), 50.0 / 1050.0);
        assert_eq!(stats.cardinality(), Some(0.1));
    }

    #[test]
    fn test_feature_freshness() {
        let now = current_timestamp();
        let fresh = FeatureFreshness::check("feat_1", now - 10, 3600);
        assert!(!fresh.is_stale);

        let stale = FeatureFreshness::check("feat_2", now - 7200, 3600);
        assert!(stale.is_stale);
        // Allow 2 second tolerance for timing differences during test execution
        assert!(
            stale.staleness_seconds >= 3599 && stale.staleness_seconds <= 3602,
            "staleness_seconds {} should be approximately 3600",
            stale.staleness_seconds
        );
    }

    #[test]
    fn test_feature_store() {
        let mut store = FeatureStore::new();

        let feature = FeatureDefinition::new("age", FeatureDataType::Integer, "users", "user");
        store.register_feature(feature);

        let set = store.create_feature_set("user_demographics", "user");
        set.add_feature("feat_1");

        assert_eq!(store.features.len(), 1);
        assert_eq!(store.feature_sets.len(), 1);
    }

    #[test]
    fn test_feature_store_search() {
        let mut store = FeatureStore::new();

        let feature1 =
            FeatureDefinition::new("user_age", FeatureDataType::Integer, "users", "user")
                .with_description("Age of the user");
        store.register_feature(feature1);

        let feature2 =
            FeatureDefinition::new("user_country", FeatureDataType::String, "users", "user");
        store.register_feature(feature2);

        let results = store.search_features("age");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "user_age");
    }

    #[test]
    fn test_feature_store_by_entity() {
        let mut store = FeatureStore::new();

        store.register_feature(FeatureDefinition::new(
            "user_age",
            FeatureDataType::Integer,
            "users",
            "user",
        ));
        store.register_feature(FeatureDefinition::new(
            "product_price",
            FeatureDataType::Float,
            "products",
            "product",
        ));

        let user_features = store.features_by_entity("user");
        assert_eq!(user_features.len(), 1);
    }

    #[test]
    fn test_feature_store_data_quality() {
        let mut store = FeatureStore::new();

        let feature = FeatureDefinition::new("test", FeatureDataType::Float, "table", "entity");
        store.register_feature(feature);

        let mut stats = FeatureStats::new("feat_1");
        stats.count = 100;
        stats.null_count = 150; // Over 50% null rate (150/250 = 60%)
        store.update_stats("feat_1", stats);

        let report = store.data_quality_report();
        assert_eq!(report.len(), 1);
        assert!(report[0].2.is_some()); // Should have an issue
    }

    #[test]
    fn test_feature_store_stale_features() {
        let mut store = FeatureStore::new();
        let now = current_timestamp();

        store.check_freshness("feat_1", now - 100, 3600); // Fresh
        store.check_freshness("feat_2", now - 7200, 3600); // Stale

        let stale = store.stale_features();
        assert_eq!(stale.len(), 1);
    }
}
