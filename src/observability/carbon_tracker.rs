//! Carbon Footprint Tracker
//!
//! Track environmental impact of compute operations, API calls,
//! and provide optimization suggestions for sustainability.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Atomic counter for unique IDs
static EMISSION_COUNTER: AtomicU64 = AtomicU64::new(0);
static REPORT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate unique emission ID
fn generate_emission_id() -> String {
    format!(
        "emission-{}",
        EMISSION_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// Generate unique report ID
fn generate_report_id() -> String {
    format!("report-{}", REPORT_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Carbon emission source type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmissionSource {
    /// LLM API call
    LlmApiCall,
    /// GPU compute
    GpuCompute,
    /// CPU compute
    CpuCompute,
    /// Data transfer
    DataTransfer,
    /// Storage operations
    Storage,
    /// Network operations
    Network,
    /// Build/compile operations
    Build,
    /// Container operations
    Container,
    /// Database queries
    Database,
    /// Other
    Other,
}

impl std::fmt::Display for EmissionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmissionSource::LlmApiCall => write!(f, "LLM API"),
            EmissionSource::GpuCompute => write!(f, "GPU Compute"),
            EmissionSource::CpuCompute => write!(f, "CPU Compute"),
            EmissionSource::DataTransfer => write!(f, "Data Transfer"),
            EmissionSource::Storage => write!(f, "Storage"),
            EmissionSource::Network => write!(f, "Network"),
            EmissionSource::Build => write!(f, "Build"),
            EmissionSource::Container => write!(f, "Container"),
            EmissionSource::Database => write!(f, "Database"),
            EmissionSource::Other => write!(f, "Other"),
        }
    }
}

/// Energy grid carbon intensity (gCO2e/kWh)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridIntensity {
    /// Very low carbon grid (e.g., Iceland, Norway)
    VeryLow,
    /// Low carbon grid (e.g., France, Sweden)
    Low,
    /// Medium carbon grid (e.g., UK, California)
    Medium,
    /// High carbon grid (e.g., US average)
    High,
    /// Very high carbon grid (e.g., China, India)
    VeryHigh,
    /// Custom intensity value
    Custom(f64),
}

impl GridIntensity {
    /// Get gCO2e per kWh
    pub fn grams_co2_per_kwh(&self) -> f64 {
        match self {
            GridIntensity::VeryLow => 20.0,
            GridIntensity::Low => 50.0,
            GridIntensity::Medium => 250.0,
            GridIntensity::High => 400.0,
            GridIntensity::VeryHigh => 600.0,
            GridIntensity::Custom(v) => *v,
        }
    }
}

impl std::fmt::Display for GridIntensity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GridIntensity::VeryLow => write!(f, "Very Low (~20g CO2e/kWh)"),
            GridIntensity::Low => write!(f, "Low (~50g CO2e/kWh)"),
            GridIntensity::Medium => write!(f, "Medium (~250g CO2e/kWh)"),
            GridIntensity::High => write!(f, "High (~400g CO2e/kWh)"),
            GridIntensity::VeryHigh => write!(f, "Very High (~600g CO2e/kWh)"),
            GridIntensity::Custom(v) => write!(f, "Custom ({}g CO2e/kWh)", v),
        }
    }
}

/// Cloud provider with carbon data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloudProvider {
    /// Amazon Web Services
    Aws,
    /// Google Cloud Platform
    Gcp,
    /// Microsoft Azure
    Azure,
    /// Self-hosted
    SelfHosted,
    /// Local development
    Local,
    /// Other
    Other,
}

impl std::fmt::Display for CloudProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloudProvider::Aws => write!(f, "AWS"),
            CloudProvider::Gcp => write!(f, "GCP"),
            CloudProvider::Azure => write!(f, "Azure"),
            CloudProvider::SelfHosted => write!(f, "Self-Hosted"),
            CloudProvider::Local => write!(f, "Local"),
            CloudProvider::Other => write!(f, "Other"),
        }
    }
}

impl CloudProvider {
    /// Get Power Usage Effectiveness (PUE) factor
    pub fn pue(&self) -> f64 {
        match self {
            CloudProvider::Gcp => 1.1,        // GCP is very efficient
            CloudProvider::Aws => 1.2,        // AWS average
            CloudProvider::Azure => 1.18,     // Azure average
            CloudProvider::SelfHosted => 1.6, // Typical data center
            CloudProvider::Local => 2.0,      // Home/office typically less efficient
            CloudProvider::Other => 1.5,      // Conservative estimate
        }
    }

    /// Whether provider offers carbon-neutral options
    pub fn has_green_option(&self) -> bool {
        match self {
            CloudProvider::Gcp => true,   // Carbon neutral since 2007
            CloudProvider::Aws => true,   // Climate pledge
            CloudProvider::Azure => true, // Carbon negative goal
            CloudProvider::SelfHosted | CloudProvider::Local | CloudProvider::Other => false,
        }
    }
}

/// LLM model type for emission estimation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmModel {
    /// GPT-4 or similar large model
    GptLarge,
    /// GPT-3.5 or similar medium model
    GptMedium,
    /// Small model (e.g., Llama 7B)
    Small,
    /// Tiny model (e.g., Phi-2)
    Tiny,
    /// Local model
    Local,
    /// Claude models
    Claude,
    /// Custom with specified parameters
    Custom,
}

impl std::fmt::Display for LlmModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmModel::GptLarge => write!(f, "GPT-4/Large"),
            LlmModel::GptMedium => write!(f, "GPT-3.5/Medium"),
            LlmModel::Small => write!(f, "Small (7B)"),
            LlmModel::Tiny => write!(f, "Tiny (<3B)"),
            LlmModel::Local => write!(f, "Local"),
            LlmModel::Claude => write!(f, "Claude"),
            LlmModel::Custom => write!(f, "Custom"),
        }
    }
}

impl LlmModel {
    /// Estimated energy per 1000 tokens (Wh)
    pub fn wh_per_1k_tokens(&self) -> f64 {
        // Rough estimates based on available research
        match self {
            LlmModel::GptLarge => 0.5,  // ~500Wh/1M tokens
            LlmModel::GptMedium => 0.1, // ~100Wh/1M tokens
            LlmModel::Small => 0.05,    // 7B models
            LlmModel::Tiny => 0.01,     // <3B models
            LlmModel::Local => 0.1,     // Varies widely
            LlmModel::Claude => 0.3,    // Estimated
            LlmModel::Custom => 0.2,    // Default estimate
        }
    }
}

/// A single emission record
#[derive(Debug, Clone)]
pub struct EmissionRecord {
    /// Unique identifier
    pub id: String,
    /// Source of emission
    pub source: EmissionSource,
    /// Carbon dioxide equivalent in grams
    pub co2e_grams: f64,
    /// Energy consumed in Wh
    pub energy_wh: f64,
    /// Timestamp
    pub timestamp: u64,
    /// Duration of operation
    pub duration: Option<Duration>,
    /// Description
    pub description: String,
    /// Associated operation
    pub operation: Option<String>,
    /// Provider used
    pub provider: Option<CloudProvider>,
    /// Region
    pub region: Option<String>,
}

impl EmissionRecord {
    /// Create a new emission record
    pub fn new(source: EmissionSource, co2e_grams: f64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: generate_emission_id(),
            source,
            co2e_grams,
            energy_wh: 0.0,
            timestamp,
            duration: None,
            description: String::new(),
            operation: None,
            provider: None,
            region: None,
        }
    }

    /// Set energy consumption
    pub fn with_energy(mut self, energy_wh: f64) -> Self {
        self.energy_wh = energy_wh;
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set operation
    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    /// Set provider
    pub fn with_provider(mut self, provider: CloudProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set region
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }
}

/// Optimization suggestion
#[derive(Debug, Clone)]
pub struct Optimization {
    /// Suggestion title
    pub title: String,
    /// Description
    pub description: String,
    /// Estimated CO2e savings in grams
    pub estimated_savings_grams: f64,
    /// Effort level to implement
    pub effort: EffortLevel,
    /// Priority
    pub priority: Priority,
    /// Category
    pub category: OptimizationCategory,
}

impl Optimization {
    /// Create a new optimization
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            estimated_savings_grams: 0.0,
            effort: EffortLevel::Low,
            priority: Priority::Medium,
            category: OptimizationCategory::Compute,
        }
    }

    /// Set estimated savings
    pub fn with_savings(mut self, grams: f64) -> Self {
        self.estimated_savings_grams = grams;
        self
    }

    /// Set effort level
    pub fn with_effort(mut self, effort: EffortLevel) -> Self {
        self.effort = effort;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set category
    pub fn with_category(mut self, category: OptimizationCategory) -> Self {
        self.category = category;
        self
    }
}

/// Effort level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffortLevel {
    /// Minimal effort
    Low,
    /// Moderate effort
    Medium,
    /// Significant effort
    High,
}

impl std::fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EffortLevel::Low => write!(f, "Low"),
            EffortLevel::Medium => write!(f, "Medium"),
            EffortLevel::High => write!(f, "High"),
        }
    }
}

/// Priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Low priority
    Low,
    /// Medium priority
    Medium,
    /// High priority
    High,
    /// Critical priority
    Critical,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Low => write!(f, "Low"),
            Priority::Medium => write!(f, "Medium"),
            Priority::High => write!(f, "High"),
            Priority::Critical => write!(f, "Critical"),
        }
    }
}

/// Optimization category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptimizationCategory {
    /// Compute optimization
    Compute,
    /// API optimization
    Api,
    /// Storage optimization
    Storage,
    /// Network optimization
    Network,
    /// Hosting optimization
    Hosting,
    /// Caching optimization
    Caching,
    /// Model selection
    ModelSelection,
}

impl std::fmt::Display for OptimizationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizationCategory::Compute => write!(f, "Compute"),
            OptimizationCategory::Api => write!(f, "API"),
            OptimizationCategory::Storage => write!(f, "Storage"),
            OptimizationCategory::Network => write!(f, "Network"),
            OptimizationCategory::Hosting => write!(f, "Hosting"),
            OptimizationCategory::Caching => write!(f, "Caching"),
            OptimizationCategory::ModelSelection => write!(f, "Model Selection"),
        }
    }
}

/// Green hosting recommendation
#[derive(Debug, Clone)]
pub struct GreenHosting {
    /// Provider name
    pub provider: String,
    /// Region
    pub region: String,
    /// Grid intensity
    pub grid_intensity: GridIntensity,
    /// Renewable energy percentage
    pub renewable_percentage: u8,
    /// Carbon neutral
    pub carbon_neutral: bool,
    /// Description
    pub description: String,
}

impl GreenHosting {
    /// Create a new green hosting recommendation
    pub fn new(provider: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            region: region.into(),
            grid_intensity: GridIntensity::Medium,
            renewable_percentage: 0,
            carbon_neutral: false,
            description: String::new(),
        }
    }

    /// Set grid intensity
    pub fn with_intensity(mut self, intensity: GridIntensity) -> Self {
        self.grid_intensity = intensity;
        self
    }

    /// Set renewable percentage
    pub fn with_renewable(mut self, percentage: u8) -> Self {
        self.renewable_percentage = percentage.min(100);
        self
    }

    /// Mark as carbon neutral
    pub fn carbon_neutral(mut self) -> Self {
        self.carbon_neutral = true;
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// Emission calculator
#[derive(Debug)]
pub struct EmissionCalculator {
    /// Grid intensity
    grid_intensity: GridIntensity,
    /// Cloud provider
    provider: CloudProvider,
}

impl Default for EmissionCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl EmissionCalculator {
    /// Create a new calculator
    pub fn new() -> Self {
        Self {
            grid_intensity: GridIntensity::Medium,
            provider: CloudProvider::Local,
        }
    }

    /// Set grid intensity
    pub fn with_intensity(mut self, intensity: GridIntensity) -> Self {
        self.grid_intensity = intensity;
        self
    }

    /// Set cloud provider
    pub fn with_provider(mut self, provider: CloudProvider) -> Self {
        self.provider = provider;
        self
    }

    /// Calculate CO2e from energy consumption
    pub fn energy_to_co2e(&self, energy_wh: f64) -> f64 {
        // Apply PUE factor
        let adjusted_energy = energy_wh * self.provider.pue();
        // Convert to kWh and multiply by grid intensity
        (adjusted_energy / 1000.0) * self.grid_intensity.grams_co2_per_kwh()
    }

    /// Calculate emissions from LLM API call
    pub fn llm_call_emission(&self, model: LlmModel, tokens: u64) -> EmissionRecord {
        let energy_wh = model.wh_per_1k_tokens() * (tokens as f64 / 1000.0);
        let co2e = self.energy_to_co2e(energy_wh);

        EmissionRecord::new(EmissionSource::LlmApiCall, co2e)
            .with_energy(energy_wh)
            .with_description(format!("{} API call with {} tokens", model, tokens))
    }

    /// Calculate emissions from CPU compute
    pub fn cpu_emission(&self, duration: Duration, cpu_power_w: f64) -> EmissionRecord {
        let hours = duration.as_secs_f64() / 3600.0;
        let energy_wh = cpu_power_w * hours;
        let co2e = self.energy_to_co2e(energy_wh);

        EmissionRecord::new(EmissionSource::CpuCompute, co2e)
            .with_energy(energy_wh)
            .with_duration(duration)
            .with_description(format!(
                "CPU compute at {}W for {:?}",
                cpu_power_w, duration
            ))
    }

    /// Calculate emissions from GPU compute
    pub fn gpu_emission(&self, duration: Duration, gpu_power_w: f64) -> EmissionRecord {
        let hours = duration.as_secs_f64() / 3600.0;
        let energy_wh = gpu_power_w * hours;
        let co2e = self.energy_to_co2e(energy_wh);

        EmissionRecord::new(EmissionSource::GpuCompute, co2e)
            .with_energy(energy_wh)
            .with_duration(duration)
            .with_description(format!(
                "GPU compute at {}W for {:?}",
                gpu_power_w, duration
            ))
    }

    /// Calculate emissions from data transfer
    pub fn data_transfer_emission(&self, bytes: u64) -> EmissionRecord {
        // Rough estimate: ~0.06 kWh per GB of data transfer
        let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let energy_wh = gb * 60.0; // 60 Wh per GB
        let co2e = self.energy_to_co2e(energy_wh);

        EmissionRecord::new(EmissionSource::DataTransfer, co2e)
            .with_energy(energy_wh)
            .with_description(format!("Data transfer: {:.2} GB", gb))
    }

    /// Calculate emissions from storage
    pub fn storage_emission(&self, gb_months: f64) -> EmissionRecord {
        // Rough estimate: ~0.7 kWh per GB per month (SSD)
        let energy_wh = gb_months * 700.0;
        let co2e = self.energy_to_co2e(energy_wh);

        EmissionRecord::new(EmissionSource::Storage, co2e)
            .with_energy(energy_wh)
            .with_description(format!("Storage: {:.2} GB-months", gb_months))
    }

    /// Calculate emissions from build operation
    pub fn build_emission(
        &self,
        duration: Duration,
        cpu_cores: u32,
        core_power_w: f64,
    ) -> EmissionRecord {
        let hours = duration.as_secs_f64() / 3600.0;
        let total_power = cpu_cores as f64 * core_power_w;
        let energy_wh = total_power * hours;
        let co2e = self.energy_to_co2e(energy_wh);

        EmissionRecord::new(EmissionSource::Build, co2e)
            .with_energy(energy_wh)
            .with_duration(duration)
            .with_description(format!("Build with {} cores for {:?}", cpu_cores, duration))
    }
}

/// Carbon footprint tracker
#[derive(Debug)]
pub struct CarbonTracker {
    /// Emission records
    records: Vec<EmissionRecord>,
    /// Calculator
    calculator: EmissionCalculator,
    /// Session start time
    _session_start: u64,
    /// Provider
    provider: CloudProvider,
    /// Region
    region: Option<String>,
}

impl Default for CarbonTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CarbonTracker {
    /// Create a new carbon tracker
    pub fn new() -> Self {
        let _session_start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            records: Vec::new(),
            calculator: EmissionCalculator::new(),
            _session_start,
            provider: CloudProvider::Local,
            region: None,
        }
    }

    /// Set calculator configuration
    pub fn with_calculator(mut self, calculator: EmissionCalculator) -> Self {
        self.calculator = calculator;
        self
    }

    /// Set provider
    pub fn with_provider(mut self, provider: CloudProvider) -> Self {
        self.provider = provider;
        self.calculator = self.calculator.with_provider(provider);
        self
    }

    /// Set region
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Record an emission
    pub fn record(&mut self, record: EmissionRecord) {
        self.records.push(record);
    }

    /// Track LLM API call
    pub fn track_llm_call(&mut self, model: LlmModel, tokens: u64) {
        let record = self
            .calculator
            .llm_call_emission(model, tokens)
            .with_provider(self.provider);
        self.record(record);
    }

    /// Track CPU compute
    pub fn track_cpu(&mut self, duration: Duration, power_w: f64) {
        let record = self
            .calculator
            .cpu_emission(duration, power_w)
            .with_provider(self.provider);
        self.record(record);
    }

    /// Track GPU compute
    pub fn track_gpu(&mut self, duration: Duration, power_w: f64) {
        let record = self
            .calculator
            .gpu_emission(duration, power_w)
            .with_provider(self.provider);
        self.record(record);
    }

    /// Track data transfer
    pub fn track_data_transfer(&mut self, bytes: u64) {
        let record = self
            .calculator
            .data_transfer_emission(bytes)
            .with_provider(self.provider);
        self.record(record);
    }

    /// Get total CO2e in grams
    pub fn total_co2e(&self) -> f64 {
        self.records.iter().map(|r| r.co2e_grams).sum()
    }

    /// Get total energy in Wh
    pub fn total_energy(&self) -> f64 {
        self.records.iter().map(|r| r.energy_wh).sum()
    }

    /// Get emissions by source
    pub fn by_source(&self) -> HashMap<EmissionSource, f64> {
        let mut result = HashMap::new();
        for record in &self.records {
            *result.entry(record.source).or_insert(0.0) += record.co2e_grams;
        }
        result
    }

    /// Get record count
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Get all records
    pub fn records(&self) -> &[EmissionRecord] {
        &self.records
    }

    /// Generate optimization suggestions
    pub fn suggest_optimizations(&self) -> Vec<Optimization> {
        let mut suggestions = Vec::new();
        let by_source = self.by_source();

        // LLM API optimizations
        if let Some(&llm_co2) = by_source.get(&EmissionSource::LlmApiCall) {
            if llm_co2 > 100.0 {
                suggestions.push(
                    Optimization::new(
                        "Use smaller models for simple tasks",
                        "Consider using GPT-3.5 or smaller models for tasks that don't require GPT-4 level capabilities",
                    )
                    .with_savings(llm_co2 * 0.5)
                    .with_category(OptimizationCategory::ModelSelection)
                    .with_priority(Priority::High)
                );

                suggestions.push(
                    Optimization::new(
                        "Implement response caching",
                        "Cache responses for repeated queries to avoid redundant API calls",
                    )
                    .with_savings(llm_co2 * 0.3)
                    .with_category(OptimizationCategory::Caching)
                    .with_effort(EffortLevel::Medium),
                );
            }
        }

        // Compute optimizations
        let compute_co2 = by_source.get(&EmissionSource::CpuCompute).unwrap_or(&0.0)
            + by_source.get(&EmissionSource::GpuCompute).unwrap_or(&0.0);

        if compute_co2 > 50.0 {
            suggestions.push(
                Optimization::new(
                    "Schedule compute during low-carbon hours",
                    "Run batch jobs during off-peak hours when renewable energy is more available",
                )
                .with_savings(compute_co2 * 0.2)
                .with_category(OptimizationCategory::Compute)
                .with_effort(EffortLevel::Medium),
            );
        }

        // Data transfer optimizations
        if let Some(&transfer_co2) = by_source.get(&EmissionSource::DataTransfer) {
            if transfer_co2 > 10.0 {
                suggestions.push(
                    Optimization::new(
                        "Compress data transfers",
                        "Use gzip or brotli compression to reduce data transfer volume",
                    )
                    .with_savings(transfer_co2 * 0.6)
                    .with_category(OptimizationCategory::Network)
                    .with_effort(EffortLevel::Low),
                );
            }
        }

        // Green hosting suggestions
        if !self.provider.has_green_option() {
            suggestions.push(
                Optimization::new(
                    "Switch to green cloud provider",
                    "Consider using GCP (carbon neutral) or AWS/Azure with renewable energy options",
                )
                .with_savings(self.total_co2e() * 0.8)
                .with_category(OptimizationCategory::Hosting)
                .with_effort(EffortLevel::High)
                .with_priority(Priority::Critical)
            );
        }

        suggestions
    }

    /// Get green hosting recommendations
    pub fn green_hosting_recommendations(&self) -> Vec<GreenHosting> {
        vec![
            GreenHosting::new("GCP", "us-central1")
                .with_intensity(GridIntensity::Low)
                .with_renewable(100)
                .carbon_neutral()
                .with_description("Carbon neutral since 2007, 100% renewable energy matching"),
            GreenHosting::new("AWS", "eu-north-1")
                .with_intensity(GridIntensity::VeryLow)
                .with_renewable(100)
                .with_description("Stockholm region runs on 100% renewable energy"),
            GreenHosting::new("Azure", "Sweden Central")
                .with_intensity(GridIntensity::VeryLow)
                .with_renewable(100)
                .with_description("Swedish data centers powered by renewable energy"),
            GreenHosting::new("AWS", "us-west-2")
                .with_intensity(GridIntensity::Low)
                .with_renewable(95)
                .with_description("Oregon region with high renewable energy mix"),
        ]
    }

    /// Generate carbon report
    pub fn generate_report(&self) -> CarbonReport {
        CarbonReport::new(self)
    }
}

/// Carbon emissions report
#[derive(Debug)]
pub struct CarbonReport {
    /// Report ID
    pub id: String,
    /// Total CO2e in grams
    pub total_co2e_grams: f64,
    /// Total energy in Wh
    pub total_energy_wh: f64,
    /// Emissions by source
    pub by_source: HashMap<EmissionSource, f64>,
    /// Number of records
    pub record_count: usize,
    /// Optimizations suggested
    pub optimizations: Vec<Optimization>,
    /// Green hosting options
    pub green_options: Vec<GreenHosting>,
    /// Equivalents for context
    pub equivalents: CarbonEquivalents,
}

impl CarbonReport {
    /// Create a new report from tracker
    pub fn new(tracker: &CarbonTracker) -> Self {
        let total_co2e_grams = tracker.total_co2e();
        Self {
            id: generate_report_id(),
            total_co2e_grams,
            total_energy_wh: tracker.total_energy(),
            by_source: tracker.by_source(),
            record_count: tracker.record_count(),
            optimizations: tracker.suggest_optimizations(),
            green_options: tracker.green_hosting_recommendations(),
            equivalents: CarbonEquivalents::from_co2e_grams(total_co2e_grams),
        }
    }

    /// Render as markdown
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str("# Carbon Footprint Report\n\n");

        // Summary
        output.push_str("## Summary\n\n");
        output.push_str(&format!(
            "- **Total CO2e**: {:.2}g ({:.4} kg)\n",
            self.total_co2e_grams,
            self.total_co2e_grams / 1000.0
        ));
        output.push_str(&format!(
            "- **Total Energy**: {:.2} Wh ({:.4} kWh)\n",
            self.total_energy_wh,
            self.total_energy_wh / 1000.0
        ));
        output.push_str(&format!(
            "- **Operations Tracked**: {}\n\n",
            self.record_count
        ));

        // Equivalents
        output.push_str("## Environmental Impact Context\n\n");
        output.push_str(&format!(
            "- {} km driven in a car\n",
            self.equivalents.car_km
        ));
        output.push_str(&format!(
            "- {} smartphone charges\n",
            self.equivalents.smartphone_charges
        ));
        output.push_str(&format!(
            "- {} hours of laptop use\n",
            self.equivalents.laptop_hours
        ));
        output.push_str(&format!(
            "- {} liters of water heated\n",
            self.equivalents.liters_water_heated
        ));
        output.push('\n');

        // By source
        output.push_str("## Emissions by Source\n\n");
        let mut sources: Vec<_> = self.by_source.iter().collect();
        sources.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (source, co2e) in sources {
            let percentage = if self.total_co2e_grams > 0.0 {
                (co2e / self.total_co2e_grams) * 100.0
            } else {
                0.0
            };
            output.push_str(&format!(
                "- **{}**: {:.2}g ({:.1}%)\n",
                source, co2e, percentage
            ));
        }
        output.push('\n');

        // Optimizations
        if !self.optimizations.is_empty() {
            output.push_str("## Optimization Suggestions\n\n");
            for opt in &self.optimizations {
                output.push_str(&format!("### {} [{}]\n\n", opt.title, opt.priority));
                output.push_str(&format!("{}\n\n", opt.description));
                output.push_str(&format!("- **Category**: {}\n", opt.category));
                output.push_str(&format!("- **Effort**: {}\n", opt.effort));
                output.push_str(&format!(
                    "- **Est. Savings**: {:.2}g CO2e\n\n",
                    opt.estimated_savings_grams
                ));
            }
        }

        // Green hosting
        output.push_str("## Green Hosting Options\n\n");
        for hosting in &self.green_options {
            output.push_str(&format!(
                "### {} - {}\n\n",
                hosting.provider, hosting.region
            ));
            if hosting.carbon_neutral {
                output.push_str("*Carbon Neutral*\n\n");
            }
            output.push_str(&format!(
                "- Renewable Energy: {}%\n",
                hosting.renewable_percentage
            ));
            output.push_str(&format!("- Grid Intensity: {}\n", hosting.grid_intensity));
            if !hosting.description.is_empty() {
                output.push_str(&format!("\n{}\n\n", hosting.description));
            }
        }

        output
    }
}

/// Carbon equivalents for context
#[derive(Debug, Clone)]
pub struct CarbonEquivalents {
    /// Kilometers driven in an average car
    pub car_km: f64,
    /// Number of smartphone charges
    pub smartphone_charges: u32,
    /// Hours of laptop use
    pub laptop_hours: f64,
    /// Liters of water heated to boiling
    pub liters_water_heated: f64,
}

impl CarbonEquivalents {
    /// Calculate equivalents from CO2e grams
    pub fn from_co2e_grams(grams: f64) -> Self {
        Self {
            // Average car emits ~120g CO2/km
            car_km: (grams / 120.0 * 100.0).round() / 100.0,
            // Smartphone charge: ~8g CO2
            smartphone_charges: (grams / 8.0).ceil() as u32,
            // Laptop use: ~30-50g CO2/hour
            laptop_hours: (grams / 40.0 * 100.0).round() / 100.0,
            // Heating water: ~50g CO2/liter
            liters_water_heated: (grams / 50.0 * 100.0).round() / 100.0,
        }
    }
}

/// Carbon budget
#[derive(Debug)]
pub struct CarbonBudget {
    /// Daily budget in grams
    pub daily_grams: f64,
    /// Weekly budget in grams
    pub weekly_grams: f64,
    /// Monthly budget in grams
    pub monthly_grams: f64,
    /// Used today
    pub used_today: f64,
    /// Used this week
    pub used_week: f64,
    /// Used this month
    pub used_month: f64,
}

impl CarbonBudget {
    /// Create a new budget
    pub fn new(daily_grams: f64) -> Self {
        Self {
            daily_grams,
            weekly_grams: daily_grams * 7.0,
            monthly_grams: daily_grams * 30.0,
            used_today: 0.0,
            used_week: 0.0,
            used_month: 0.0,
        }
    }

    /// Add usage
    pub fn add_usage(&mut self, grams: f64) {
        self.used_today += grams;
        self.used_week += grams;
        self.used_month += grams;
    }

    /// Check if over daily budget
    pub fn over_daily(&self) -> bool {
        self.used_today > self.daily_grams
    }

    /// Get daily usage percentage
    pub fn daily_percentage(&self) -> f64 {
        if self.daily_grams > 0.0 {
            (self.used_today / self.daily_grams) * 100.0
        } else {
            0.0
        }
    }

    /// Get remaining daily budget
    pub fn remaining_today(&self) -> f64 {
        (self.daily_grams - self.used_today).max(0.0)
    }

    /// Reset daily usage
    pub fn reset_daily(&mut self) {
        self.used_today = 0.0;
    }

    /// Reset weekly usage
    pub fn reset_weekly(&mut self) {
        self.used_week = 0.0;
    }

    /// Reset monthly usage
    pub fn reset_monthly(&mut self) {
        self.used_month = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emission_source_display() {
        assert_eq!(format!("{}", EmissionSource::LlmApiCall), "LLM API");
        assert_eq!(format!("{}", EmissionSource::GpuCompute), "GPU Compute");
    }

    #[test]
    fn test_grid_intensity_values() {
        assert_eq!(GridIntensity::VeryLow.grams_co2_per_kwh(), 20.0);
        assert_eq!(GridIntensity::Low.grams_co2_per_kwh(), 50.0);
        assert_eq!(GridIntensity::Custom(100.0).grams_co2_per_kwh(), 100.0);
    }

    #[test]
    fn test_cloud_provider_pue() {
        assert!(CloudProvider::Gcp.pue() < CloudProvider::Local.pue());
        assert!(CloudProvider::Aws.pue() < CloudProvider::SelfHosted.pue());
    }

    #[test]
    fn test_cloud_provider_green_option() {
        assert!(CloudProvider::Gcp.has_green_option());
        assert!(CloudProvider::Aws.has_green_option());
        assert!(!CloudProvider::Local.has_green_option());
    }

    #[test]
    fn test_llm_model_energy() {
        assert!(LlmModel::GptLarge.wh_per_1k_tokens() > LlmModel::Tiny.wh_per_1k_tokens());
        assert!(LlmModel::GptMedium.wh_per_1k_tokens() < LlmModel::GptLarge.wh_per_1k_tokens());
    }

    #[test]
    fn test_emission_record_creation() {
        let record = EmissionRecord::new(EmissionSource::LlmApiCall, 10.0)
            .with_energy(5.0)
            .with_description("Test call");

        assert_eq!(record.source, EmissionSource::LlmApiCall);
        assert_eq!(record.co2e_grams, 10.0);
        assert_eq!(record.energy_wh, 5.0);
    }

    #[test]
    fn test_optimization_creation() {
        let opt = Optimization::new("Test", "Description")
            .with_savings(100.0)
            .with_effort(EffortLevel::High)
            .with_priority(Priority::Critical);

        assert_eq!(opt.title, "Test");
        assert_eq!(opt.estimated_savings_grams, 100.0);
        assert_eq!(opt.effort, EffortLevel::High);
        assert_eq!(opt.priority, Priority::Critical);
    }

    #[test]
    fn test_green_hosting_creation() {
        let hosting = GreenHosting::new("GCP", "us-central1")
            .with_renewable(100)
            .carbon_neutral();

        assert_eq!(hosting.provider, "GCP");
        assert_eq!(hosting.renewable_percentage, 100);
        assert!(hosting.carbon_neutral);
    }

    #[test]
    fn test_emission_calculator_energy_to_co2e() {
        let calc = EmissionCalculator::new()
            .with_intensity(GridIntensity::Medium)
            .with_provider(CloudProvider::Local);

        let co2e = calc.energy_to_co2e(1000.0); // 1 kWh

        // 1 kWh * 2.0 PUE * 250 gCO2/kWh = 500g
        assert!((co2e - 500.0).abs() < 0.01);
    }

    #[test]
    fn test_emission_calculator_llm_call() {
        let calc = EmissionCalculator::new().with_intensity(GridIntensity::Medium);

        let record = calc.llm_call_emission(LlmModel::GptLarge, 1000);

        assert!(record.energy_wh > 0.0);
        assert!(record.co2e_grams > 0.0);
        assert_eq!(record.source, EmissionSource::LlmApiCall);
    }

    #[test]
    fn test_emission_calculator_cpu() {
        let calc = EmissionCalculator::new();
        let record = calc.cpu_emission(Duration::from_secs(3600), 100.0);

        assert!(record.energy_wh > 0.0);
        assert!(record.co2e_grams > 0.0);
        assert_eq!(record.source, EmissionSource::CpuCompute);
    }

    #[test]
    fn test_emission_calculator_gpu() {
        let calc = EmissionCalculator::new();
        let record = calc.gpu_emission(Duration::from_secs(1800), 300.0);

        assert!(record.energy_wh > 0.0);
        assert!(record.co2e_grams > 0.0);
        assert_eq!(record.source, EmissionSource::GpuCompute);
    }

    #[test]
    fn test_emission_calculator_data_transfer() {
        let calc = EmissionCalculator::new();
        let record = calc.data_transfer_emission(1024 * 1024 * 1024); // 1 GB

        assert!(record.energy_wh > 0.0);
        assert!(record.co2e_grams > 0.0);
    }

    #[test]
    fn test_carbon_tracker_creation() {
        let tracker = CarbonTracker::new();
        assert_eq!(tracker.record_count(), 0);
        assert_eq!(tracker.total_co2e(), 0.0);
    }

    #[test]
    fn test_carbon_tracker_track_llm() {
        let mut tracker = CarbonTracker::new();
        tracker.track_llm_call(LlmModel::GptLarge, 1000);

        assert_eq!(tracker.record_count(), 1);
        assert!(tracker.total_co2e() > 0.0);
    }

    #[test]
    fn test_carbon_tracker_track_multiple() {
        let mut tracker = CarbonTracker::new();
        tracker.track_llm_call(LlmModel::GptLarge, 1000);
        tracker.track_cpu(Duration::from_secs(60), 50.0);
        tracker.track_data_transfer(1024 * 1024);

        assert_eq!(tracker.record_count(), 3);
    }

    #[test]
    fn test_carbon_tracker_by_source() {
        let mut tracker = CarbonTracker::new();
        tracker.track_llm_call(LlmModel::GptLarge, 1000);
        tracker.track_cpu(Duration::from_secs(60), 50.0);

        let by_source = tracker.by_source();

        assert!(by_source.contains_key(&EmissionSource::LlmApiCall));
        assert!(by_source.contains_key(&EmissionSource::CpuCompute));
    }

    #[test]
    fn test_carbon_tracker_suggest_optimizations() {
        let mut tracker = CarbonTracker::new();

        // Generate enough LLM emissions to trigger suggestions
        for _ in 0..100 {
            tracker.track_llm_call(LlmModel::GptLarge, 10000);
        }

        let suggestions = tracker.suggest_optimizations();

        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_carbon_tracker_green_hosting_recommendations() {
        let tracker = CarbonTracker::new();
        let recommendations = tracker.green_hosting_recommendations();

        assert!(!recommendations.is_empty());
        assert!(recommendations.iter().any(|r| r.carbon_neutral));
    }

    #[test]
    fn test_carbon_report_generation() {
        let mut tracker = CarbonTracker::new();
        tracker.track_llm_call(LlmModel::GptMedium, 500);

        let report = tracker.generate_report();

        assert!(report.total_co2e_grams > 0.0);
        assert_eq!(report.record_count, 1);
    }

    #[test]
    fn test_carbon_report_markdown() {
        let mut tracker = CarbonTracker::new();
        tracker.track_llm_call(LlmModel::GptMedium, 500);

        let report = tracker.generate_report();
        let md = report.to_markdown();

        assert!(md.contains("# Carbon Footprint Report"));
        assert!(md.contains("Summary"));
    }

    #[test]
    fn test_carbon_equivalents() {
        let equiv = CarbonEquivalents::from_co2e_grams(120.0);

        assert!((equiv.car_km - 1.0).abs() < 0.01);
        assert!(equiv.smartphone_charges > 0);
    }

    #[test]
    fn test_carbon_budget_creation() {
        let budget = CarbonBudget::new(100.0);

        assert_eq!(budget.daily_grams, 100.0);
        assert_eq!(budget.weekly_grams, 700.0);
        assert_eq!(budget.used_today, 0.0);
    }

    #[test]
    fn test_carbon_budget_add_usage() {
        let mut budget = CarbonBudget::new(100.0);
        budget.add_usage(50.0);

        assert_eq!(budget.used_today, 50.0);
        assert_eq!(budget.daily_percentage(), 50.0);
        assert!(!budget.over_daily());
    }

    #[test]
    fn test_carbon_budget_over_daily() {
        let mut budget = CarbonBudget::new(100.0);
        budget.add_usage(150.0);

        assert!(budget.over_daily());
        assert_eq!(budget.remaining_today(), 0.0);
    }

    #[test]
    fn test_carbon_budget_reset() {
        let mut budget = CarbonBudget::new(100.0);
        budget.add_usage(50.0);
        budget.reset_daily();

        assert_eq!(budget.used_today, 0.0);
        assert_eq!(budget.used_week, 50.0); // Week not reset
    }

    #[test]
    fn test_unique_emission_ids() {
        let r1 = EmissionRecord::new(EmissionSource::Other, 0.0);
        let r2 = EmissionRecord::new(EmissionSource::Other, 0.0);

        assert_ne!(r1.id, r2.id);
    }

    #[test]
    fn test_unique_report_ids() {
        let tracker = CarbonTracker::new();
        let r1 = tracker.generate_report();
        let r2 = tracker.generate_report();

        assert_ne!(r1.id, r2.id);
    }

    #[test]
    fn test_effort_level_ordering() {
        assert!(EffortLevel::Low < EffortLevel::Medium);
        assert!(EffortLevel::Medium < EffortLevel::High);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Low < Priority::Medium);
        assert!(Priority::High < Priority::Critical);
    }

    #[test]
    fn test_green_hosting_renewable_clamping() {
        let hosting = GreenHosting::new("Test", "region").with_renewable(150); // Over 100

        assert_eq!(hosting.renewable_percentage, 100);
    }

    #[test]
    fn test_optimization_category_display() {
        assert_eq!(format!("{}", OptimizationCategory::Compute), "Compute");
        assert_eq!(
            format!("{}", OptimizationCategory::ModelSelection),
            "Model Selection"
        );
    }

    #[test]
    fn test_cloud_provider_display() {
        assert_eq!(format!("{}", CloudProvider::Aws), "AWS");
        assert_eq!(format!("{}", CloudProvider::Gcp), "GCP");
    }

    #[test]
    fn test_llm_model_display() {
        assert_eq!(format!("{}", LlmModel::GptLarge), "GPT-4/Large");
        assert_eq!(format!("{}", LlmModel::Claude), "Claude");
    }

    #[test]
    fn test_emission_record_with_all_fields() {
        let record = EmissionRecord::new(EmissionSource::Build, 50.0)
            .with_energy(100.0)
            .with_duration(Duration::from_secs(300))
            .with_description("Build project")
            .with_operation("cargo build")
            .with_provider(CloudProvider::Local)
            .with_region("local");

        assert!(record.duration.is_some());
        assert!(record.operation.is_some());
        assert!(record.provider.is_some());
        assert!(record.region.is_some());
    }

    #[test]
    fn test_carbon_tracker_with_config() {
        let tracker = CarbonTracker::new()
            .with_provider(CloudProvider::Gcp)
            .with_region("us-central1");

        assert_eq!(tracker.provider, CloudProvider::Gcp);
        assert_eq!(tracker.region, Some("us-central1".to_string()));
    }

    #[test]
    fn test_build_emission() {
        let calc = EmissionCalculator::new();
        let record = calc.build_emission(Duration::from_secs(60), 4, 15.0);

        assert!(record.energy_wh > 0.0);
        assert!(record.co2e_grams > 0.0);
        assert_eq!(record.source, EmissionSource::Build);
    }

    #[test]
    fn test_storage_emission() {
        let calc = EmissionCalculator::new();
        let record = calc.storage_emission(10.0); // 10 GB-months

        assert!(record.energy_wh > 0.0);
        assert!(record.co2e_grams > 0.0);
        assert_eq!(record.source, EmissionSource::Storage);
    }
}
