//! Technical Debt Management System
//!
//! Provides debt quantification, prioritization algorithms,
//! refactoring roadmap generation, and code age/churn correlation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::util::{current_timestamp_secs, save_json_pretty};

static DEBT_COUNTER: AtomicU64 = AtomicU64::new(1);
static ROADMAP_COUNTER: AtomicU64 = AtomicU64::new(1);

// ============================================================================
// Debt Quantification
// ============================================================================

/// Type of technical debt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DebtType {
    /// Code duplication
    Duplication,
    /// Complex code (high cyclomatic complexity)
    Complexity,
    /// Missing tests
    TestCoverage,
    /// Outdated dependencies
    Dependencies,
    /// Poor documentation
    Documentation,
    /// Code style violations
    CodeStyle,
    /// Architecture violations
    Architecture,
    /// Security vulnerabilities
    Security,
    /// Performance issues
    Performance,
    /// Dead code
    DeadCode,
}

impl DebtType {
    pub fn default_interest_rate(&self) -> f64 {
        match self {
            DebtType::Security => 0.15,      // 15% - high urgency
            DebtType::Architecture => 0.10,  // 10% - compounds quickly
            DebtType::Complexity => 0.08,    // 8% - makes changes harder
            DebtType::Dependencies => 0.07,  // 7% - security & maintenance
            DebtType::Duplication => 0.06,   // 6% - multiplies bugs
            DebtType::TestCoverage => 0.05,  // 5% - increases risk
            DebtType::Performance => 0.04,   // 4% - user impact
            DebtType::Documentation => 0.03, // 3% - onboarding cost
            DebtType::CodeStyle => 0.02,     // 2% - readability
            DebtType::DeadCode => 0.01,      // 1% - confusion
        }
    }

    pub fn severity_weight(&self) -> f64 {
        match self {
            DebtType::Security => 5.0,
            DebtType::Architecture => 4.0,
            DebtType::Complexity => 3.5,
            DebtType::Dependencies => 3.0,
            DebtType::Duplication => 2.5,
            DebtType::TestCoverage => 2.5,
            DebtType::Performance => 2.0,
            DebtType::Documentation => 1.5,
            DebtType::CodeStyle => 1.0,
            DebtType::DeadCode => 0.5,
        }
    }
}

/// Debt severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum DebtSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl DebtSeverity {
    pub fn multiplier(&self) -> f64 {
        match self {
            DebtSeverity::Low => 1.0,
            DebtSeverity::Medium => 2.0,
            DebtSeverity::High => 4.0,
            DebtSeverity::Critical => 8.0,
        }
    }
}

/// Technical debt item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtItem {
    /// Unique ID
    pub id: String,
    /// Debt type
    pub debt_type: DebtType,
    /// Severity
    pub severity: DebtSeverity,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Affected files
    pub files: Vec<PathBuf>,
    /// Estimated fix time (hours)
    pub estimated_hours: f64,
    /// Hourly cost (dollars)
    pub hourly_cost: f64,
    /// Interest rate (per month)
    pub interest_rate: f64,
    /// Created timestamp
    pub created_at: u64,
    /// Age (days since creation)
    pub age_days: u64,
    /// Tags
    pub tags: Vec<String>,
}

impl DebtItem {
    pub fn new(debt_type: DebtType, title: impl Into<String>) -> Self {
        let id = format!("debt_{}", DEBT_COUNTER.fetch_add(1, Ordering::SeqCst));
        let now = current_timestamp_secs();
        Self {
            id,
            debt_type,
            severity: DebtSeverity::Medium,
            title: title.into(),
            description: String::new(),
            files: Vec::new(),
            estimated_hours: 1.0,
            hourly_cost: 100.0, // Default $100/hour
            interest_rate: debt_type.default_interest_rate(),
            created_at: now,
            age_days: 0,
            tags: Vec::new(),
        }
    }

    pub fn with_severity(mut self, severity: DebtSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.files.push(file.into());
        self
    }

    pub fn with_estimate(mut self, hours: f64) -> Self {
        self.estimated_hours = hours;
        self
    }

    pub fn with_age(mut self, days: u64) -> Self {
        self.age_days = days;
        self
    }

    /// Calculate the cost to fix now
    pub fn fix_cost(&self) -> f64 {
        self.estimated_hours * self.hourly_cost
    }

    /// Calculate the current total cost with accrued interest
    pub fn total_cost(&self) -> f64 {
        let base_cost = self.fix_cost();
        let months = self.age_days as f64 / 30.0;
        base_cost * (1.0 + self.interest_rate).powf(months)
    }

    /// Calculate monthly interest cost
    pub fn monthly_interest(&self) -> f64 {
        self.total_cost() * self.interest_rate
    }

    /// Calculate priority score (higher = more urgent)
    pub fn priority_score(&self) -> f64 {
        let severity_factor = self.severity.multiplier();
        let type_weight = self.debt_type.severity_weight();
        let age_factor = 1.0 + (self.age_days as f64 / 365.0);
        let cost_factor = (self.total_cost() / 1000.0).min(10.0);

        severity_factor * type_weight * age_factor * cost_factor
    }
}

/// Debt metrics for a file or module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtMetrics {
    /// Total debt items
    pub total_items: usize,
    /// Total fix cost
    pub total_fix_cost: f64,
    /// Total current cost (with interest)
    pub total_current_cost: f64,
    /// Monthly interest
    pub monthly_interest: f64,
    /// Breakdown by type
    pub by_type: HashMap<DebtType, usize>,
    /// Breakdown by severity
    pub by_severity: HashMap<DebtSeverity, usize>,
    /// Average priority score
    pub avg_priority: f64,
}

impl DebtMetrics {
    pub fn calculate(items: &[DebtItem]) -> Self {
        let total_items = items.len();
        let total_fix_cost: f64 = items.iter().map(|i| i.fix_cost()).sum();
        let total_current_cost: f64 = items.iter().map(|i| i.total_cost()).sum();
        let monthly_interest: f64 = items.iter().map(|i| i.monthly_interest()).sum();

        let mut by_type: HashMap<DebtType, usize> = HashMap::new();
        let mut by_severity: HashMap<DebtSeverity, usize> = HashMap::new();

        for item in items {
            *by_type.entry(item.debt_type).or_default() += 1;
            *by_severity.entry(item.severity).or_default() += 1;
        }

        let avg_priority = if total_items > 0 {
            items.iter().map(|i| i.priority_score()).sum::<f64>() / total_items as f64
        } else {
            0.0
        };

        Self {
            total_items,
            total_fix_cost,
            total_current_cost,
            monthly_interest,
            by_type,
            by_severity,
            avg_priority,
        }
    }
}

// ============================================================================
// Prioritization Algorithms
// ============================================================================

/// Prioritization strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrioritizationStrategy {
    /// Risk × Impact × Effort (WSJF-style)
    RiskImpactEffort,
    /// Highest cost first
    CostFirst,
    /// Highest interest rate first
    InterestFirst,
    /// Oldest debt first
    AgeFirst,
    /// Quick wins (low effort, high impact)
    QuickWins,
    /// Security first
    SecurityFirst,
}

/// Prioritized debt item with scoring
#[derive(Debug, Clone)]
pub struct PrioritizedItem {
    /// Original debt item
    pub item: DebtItem,
    /// Priority score
    pub score: f64,
    /// Risk score (0-10)
    pub risk: f64,
    /// Impact score (0-10)
    pub impact: f64,
    /// Effort score (0-10, lower is better)
    pub effort: f64,
}

/// Debt prioritizer
#[derive(Debug, Clone)]
pub struct DebtPrioritizer {
    strategy: PrioritizationStrategy,
}

impl DebtPrioritizer {
    pub fn new(strategy: PrioritizationStrategy) -> Self {
        Self { strategy }
    }

    pub fn prioritize(&self, items: &[DebtItem]) -> Vec<PrioritizedItem> {
        let mut prioritized: Vec<_> = items.iter().map(|item| self.score_item(item)).collect();

        prioritized.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        prioritized
    }

    fn score_item(&self, item: &DebtItem) -> PrioritizedItem {
        let risk = self.calculate_risk(item);
        let impact = self.calculate_impact(item);
        let effort = self.calculate_effort(item);

        let score = match self.strategy {
            PrioritizationStrategy::RiskImpactEffort => (risk * impact) / effort.max(0.1),
            PrioritizationStrategy::CostFirst => item.total_cost() / 1000.0,
            PrioritizationStrategy::InterestFirst => item.interest_rate * 100.0,
            PrioritizationStrategy::AgeFirst => item.age_days as f64,
            PrioritizationStrategy::QuickWins => impact / effort.max(0.1),
            PrioritizationStrategy::SecurityFirst => {
                if item.debt_type == DebtType::Security {
                    1000.0 + item.severity.multiplier()
                } else {
                    item.priority_score()
                }
            }
        };

        PrioritizedItem {
            item: item.clone(),
            score,
            risk,
            impact,
            effort,
        }
    }

    fn calculate_risk(&self, item: &DebtItem) -> f64 {
        let base_risk: f64 = match item.severity {
            DebtSeverity::Critical => 9.0,
            DebtSeverity::High => 7.0,
            DebtSeverity::Medium => 4.0,
            DebtSeverity::Low => 2.0,
        };

        let type_modifier: f64 = match item.debt_type {
            DebtType::Security => 1.5,
            DebtType::Architecture => 1.3,
            _ => 1.0,
        };

        (base_risk * type_modifier).min(10.0)
    }

    fn calculate_impact(&self, item: &DebtItem) -> f64 {
        let file_count = item.files.len() as f64;
        let type_impact = item.debt_type.severity_weight();
        let cost_impact = (item.total_cost() / 500.0).min(5.0);

        ((file_count * 0.5) + type_impact + cost_impact).min(10.0)
    }

    fn calculate_effort(&self, item: &DebtItem) -> f64 {
        // Convert hours to 1-10 scale
        (item.estimated_hours / 8.0).clamp(1.0, 10.0)
    }
}

// ============================================================================
// Refactoring Roadmap
// ============================================================================

/// Roadmap phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadmapPhase {
    /// Phase name
    pub name: String,
    /// Phase number
    pub phase_number: u32,
    /// Debt items to address
    pub items: Vec<String>, // Debt IDs
    /// Estimated total hours
    pub estimated_hours: f64,
    /// Estimated cost
    pub estimated_cost: f64,
    /// Expected savings (interest reduction)
    pub expected_savings: f64,
    /// Target completion (days from start)
    pub target_days: u32,
}

impl RoadmapPhase {
    pub fn new(name: impl Into<String>, phase_number: u32) -> Self {
        Self {
            name: name.into(),
            phase_number,
            items: Vec::new(),
            estimated_hours: 0.0,
            estimated_cost: 0.0,
            expected_savings: 0.0,
            target_days: 0,
        }
    }

    pub fn add_item(&mut self, item: &DebtItem) {
        self.items.push(item.id.clone());
        self.estimated_hours += item.estimated_hours;
        self.estimated_cost += item.fix_cost();
        self.expected_savings += item.monthly_interest() * 12.0; // Annual savings
    }

    pub fn roi(&self) -> f64 {
        if self.estimated_cost == 0.0 {
            0.0
        } else {
            self.expected_savings / self.estimated_cost
        }
    }
}

/// Refactoring roadmap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringRoadmap {
    /// Roadmap ID
    pub id: String,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Phases
    pub phases: Vec<RoadmapPhase>,
    /// Total estimated hours
    pub total_hours: f64,
    /// Total estimated cost
    pub total_cost: f64,
    /// Total expected annual savings
    pub annual_savings: f64,
    /// Created timestamp
    pub created_at: u64,
}

impl RefactoringRoadmap {
    pub fn new(title: impl Into<String>) -> Self {
        let id = format!("roadmap_{}", ROADMAP_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            title: title.into(),
            description: String::new(),
            phases: Vec::new(),
            total_hours: 0.0,
            total_cost: 0.0,
            annual_savings: 0.0,
            created_at: current_timestamp_secs(),
        }
    }

    pub fn add_phase(&mut self, phase: RoadmapPhase) {
        self.total_hours += phase.estimated_hours;
        self.total_cost += phase.estimated_cost;
        self.annual_savings += phase.expected_savings;
        self.phases.push(phase);
    }

    pub fn payback_months(&self) -> f64 {
        if self.annual_savings == 0.0 {
            f64::INFINITY
        } else {
            self.total_cost / (self.annual_savings / 12.0)
        }
    }
}

/// Roadmap generator
#[derive(Debug, Clone)]
pub struct RoadmapGenerator {
    /// Maximum hours per phase
    pub max_hours_per_phase: f64,
    /// Number of phases
    pub num_phases: u32,
}

impl RoadmapGenerator {
    pub fn new() -> Self {
        Self {
            max_hours_per_phase: 80.0, // 2 weeks
            num_phases: 4,
        }
    }

    pub fn with_max_hours(mut self, hours: f64) -> Self {
        self.max_hours_per_phase = hours;
        self
    }

    pub fn generate(&self, title: &str, prioritized: &[PrioritizedItem]) -> RefactoringRoadmap {
        let mut roadmap = RefactoringRoadmap::new(title);
        let mut remaining: Vec<_> = prioritized.to_vec();

        for phase_num in 1..=self.num_phases {
            if remaining.is_empty() {
                break;
            }

            let phase_name = match phase_num {
                1 => "Critical Fixes".to_string(),
                2 => "High Priority Items".to_string(),
                3 => "Medium Priority Items".to_string(),
                _ => format!("Phase {}", phase_num),
            };

            let mut phase = RoadmapPhase::new(phase_name, phase_num);
            phase.target_days = phase_num * 14; // 2 weeks per phase

            let mut hours_used = 0.0;
            let mut items_to_remove = Vec::new();

            for (idx, prioritized_item) in remaining.iter().enumerate() {
                if hours_used + prioritized_item.item.estimated_hours <= self.max_hours_per_phase {
                    phase.add_item(&prioritized_item.item);
                    hours_used += prioritized_item.item.estimated_hours;
                    items_to_remove.push(idx);
                }
            }

            // Remove items in reverse order to maintain indices
            for idx in items_to_remove.into_iter().rev() {
                remaining.remove(idx);
            }

            if !phase.items.is_empty() {
                roadmap.add_phase(phase);
            }
        }

        roadmap
    }
}

impl Default for RoadmapGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Code Age & Churn Correlation
// ============================================================================

/// File statistics for age/churn analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStats {
    /// File path
    pub path: PathBuf,
    /// First commit timestamp
    pub created_at: u64,
    /// Last modified timestamp
    pub last_modified: u64,
    /// Total commits
    pub total_commits: u32,
    /// Unique authors
    pub unique_authors: u32,
    /// Lines added (total over history)
    pub lines_added: u32,
    /// Lines deleted (total over history)
    pub lines_deleted: u32,
    /// Current line count
    pub current_lines: u32,
    /// Bug fix commits
    pub bug_fixes: u32,
}

impl FileStats {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            created_at: current_timestamp_secs(),
            last_modified: current_timestamp_secs(),
            total_commits: 0,
            unique_authors: 1,
            lines_added: 0,
            lines_deleted: 0,
            current_lines: 0,
            bug_fixes: 0,
        }
    }

    /// Calculate age in days
    pub fn age_days(&self) -> u64 {
        (current_timestamp_secs() - self.created_at) / 86400
    }

    /// Calculate churn rate (changes per month)
    pub fn churn_rate(&self) -> f64 {
        let age_months = self.age_days() as f64 / 30.0;
        if age_months == 0.0 {
            0.0
        } else {
            self.total_commits as f64 / age_months
        }
    }

    /// Calculate instability index (bug fixes / total commits)
    pub fn instability_index(&self) -> f64 {
        if self.total_commits == 0 {
            0.0
        } else {
            self.bug_fixes as f64 / self.total_commits as f64
        }
    }

    /// Calculate hotspot score (high churn + high bugs)
    pub fn hotspot_score(&self) -> f64 {
        self.churn_rate() * (1.0 + self.instability_index()) * (self.unique_authors as f64 / 2.0)
    }
}

/// Correlation analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationResult {
    /// Correlation type
    pub correlation_type: String,
    /// Correlation coefficient (-1 to 1)
    pub coefficient: f64,
    /// Statistical significance
    pub p_value: f64,
    /// Sample size
    pub sample_size: usize,
    /// Interpretation
    pub interpretation: String,
}

impl CorrelationResult {
    pub fn new(correlation_type: impl Into<String>, coefficient: f64, sample_size: usize) -> Self {
        let interpretation = if coefficient.abs() < 0.3 {
            "Weak correlation"
        } else if coefficient.abs() < 0.7 {
            "Moderate correlation"
        } else {
            "Strong correlation"
        };

        Self {
            correlation_type: correlation_type.into(),
            coefficient,
            p_value: 0.05, // Simplified
            sample_size,
            interpretation: interpretation.to_string(),
        }
    }
}

/// Churn analyzer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChurnAnalyzer {
    /// File statistics
    pub files: HashMap<PathBuf, FileStats>,
    /// Debt items for correlation
    pub debt_items: Vec<DebtItem>,
}

impl ChurnAnalyzer {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            debt_items: Vec::new(),
        }
    }

    pub fn add_file(&mut self, stats: FileStats) {
        self.files.insert(stats.path.clone(), stats);
    }

    pub fn add_debt(&mut self, item: DebtItem) {
        self.debt_items.push(item);
    }

    pub fn hotspots(&self, limit: usize) -> Vec<&FileStats> {
        let mut sorted: Vec<_> = self.files.values().collect();
        sorted.sort_by(|a, b| {
            b.hotspot_score()
                .partial_cmp(&a.hotspot_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(limit);
        sorted
    }

    pub fn high_churn_files(&self, threshold: f64) -> Vec<&FileStats> {
        self.files
            .values()
            .filter(|f| f.churn_rate() > threshold)
            .collect()
    }

    pub fn correlate_age_debt(&self) -> CorrelationResult {
        // Simplified correlation calculation
        // In a real implementation, this would use proper statistical methods

        let debt_by_file: HashMap<PathBuf, usize> = self
            .debt_items
            .iter()
            .flat_map(|d| d.files.iter().cloned())
            .fold(HashMap::new(), |mut map, file| {
                *map.entry(file).or_default() += 1;
                map
            });

        let paired_data: Vec<(f64, f64)> = self
            .files
            .iter()
            .filter_map(|(path, stats)| {
                debt_by_file
                    .get(path)
                    .map(|&debt_count| (stats.age_days() as f64, debt_count as f64))
            })
            .collect();

        if paired_data.is_empty() {
            return CorrelationResult::new("age_debt", 0.0, 0);
        }

        let coefficient = Self::pearson_correlation(&paired_data);
        CorrelationResult::new("age_debt", coefficient, paired_data.len())
    }

    pub fn correlate_churn_debt(&self) -> CorrelationResult {
        let debt_by_file: HashMap<PathBuf, usize> = self
            .debt_items
            .iter()
            .flat_map(|d| d.files.iter().cloned())
            .fold(HashMap::new(), |mut map, file| {
                *map.entry(file).or_default() += 1;
                map
            });

        let paired_data: Vec<(f64, f64)> = self
            .files
            .iter()
            .filter_map(|(path, stats)| {
                debt_by_file
                    .get(path)
                    .map(|&debt_count| (stats.churn_rate(), debt_count as f64))
            })
            .collect();

        if paired_data.is_empty() {
            return CorrelationResult::new("churn_debt", 0.0, 0);
        }

        let coefficient = Self::pearson_correlation(&paired_data);
        CorrelationResult::new("churn_debt", coefficient, paired_data.len())
    }

    fn pearson_correlation(data: &[(f64, f64)]) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        let n = data.len() as f64;
        let sum_x: f64 = data.iter().map(|(x, _)| x).sum();
        let sum_y: f64 = data.iter().map(|(_, y)| y).sum();
        let sum_xy: f64 = data.iter().map(|(x, y)| x * y).sum();
        let sum_x2: f64 = data.iter().map(|(x, _)| x * x).sum();
        let sum_y2: f64 = data.iter().map(|(_, y)| y * y).sum();

        let numerator = n * sum_xy - sum_x * sum_y;
        let denominator = ((n * sum_x2 - sum_x * sum_x) * (n * sum_y2 - sum_y * sum_y)).sqrt();

        if denominator == 0.0 {
            0.0
        } else {
            numerator / denominator
        }
    }

    pub fn generate_report(&self) -> ChurnReport {
        let hotspots = self.hotspots(10);
        let high_churn = self.high_churn_files(2.0);

        ChurnReport {
            total_files: self.files.len(),
            total_commits: self.files.values().map(|f| f.total_commits).sum(),
            hotspot_count: hotspots.len(),
            high_churn_count: high_churn.len(),
            age_debt_correlation: self.correlate_age_debt(),
            churn_debt_correlation: self.correlate_churn_debt(),
        }
    }
}

impl Default for ChurnAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Churn analysis report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChurnReport {
    /// Total files analyzed
    pub total_files: usize,
    /// Total commits analyzed
    pub total_commits: u32,
    /// Number of hotspots
    pub hotspot_count: usize,
    /// Number of high churn files
    pub high_churn_count: usize,
    /// Age-debt correlation
    pub age_debt_correlation: CorrelationResult,
    /// Churn-debt correlation
    pub churn_debt_correlation: CorrelationResult,
}

// ============================================================================
// Debt Tracker
// ============================================================================

/// Main debt tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtTracker {
    /// All debt items
    pub items: Vec<DebtItem>,
    /// Churn analyzer
    pub churn_analyzer: ChurnAnalyzer,
    /// Roadmaps
    pub roadmaps: Vec<RefactoringRoadmap>,
}

impl DebtTracker {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            churn_analyzer: ChurnAnalyzer::new(),
            roadmaps: Vec::new(),
        }
    }

    pub fn add_debt(&mut self, item: DebtItem) {
        self.churn_analyzer.add_debt(item.clone());
        self.items.push(item);
    }

    pub fn metrics(&self) -> DebtMetrics {
        DebtMetrics::calculate(&self.items)
    }

    pub fn prioritize(&self, strategy: PrioritizationStrategy) -> Vec<PrioritizedItem> {
        let prioritizer = DebtPrioritizer::new(strategy);
        prioritizer.prioritize(&self.items)
    }

    pub fn generate_roadmap(
        &mut self,
        title: &str,
        strategy: PrioritizationStrategy,
    ) -> &RefactoringRoadmap {
        let prioritized = self.prioritize(strategy);
        let generator = RoadmapGenerator::new();
        let roadmap = generator.generate(title, &prioritized);
        self.roadmaps.push(roadmap);
        self.roadmaps.last().expect("roadmap was just pushed")
    }

    pub fn critical_items(&self) -> Vec<&DebtItem> {
        self.items
            .iter()
            .filter(|i| i.severity == DebtSeverity::Critical)
            .collect()
    }

    pub fn security_debt(&self) -> Vec<&DebtItem> {
        self.items
            .iter()
            .filter(|i| i.debt_type == DebtType::Security)
            .collect()
    }

    /// Save tracker state to a JSON file.
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        save_json_pretty(self, path)
    }

    /// Load tracker state from a JSON file.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let tracker: Self = serde_json::from_str(&data)?;
        Ok(tracker)
    }

    /// Default persistence path under a project root.
    pub fn default_path(project_root: &std::path::Path) -> PathBuf {
        project_root.join(".selfware").join("tech_debt.json")
    }
}

impl Default for DebtTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/analysis/tech_debt/tech_debt_test.rs"]
mod tests;
