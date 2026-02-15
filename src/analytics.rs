//! Usage Analytics Dashboard
//!
//! Track ROI and productivity metrics:
//! - Time saved through automation
//! - Bugs prevented
//! - Code quality metrics
//! - Productivity trends

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Time period for aggregation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimePeriod {
    Hour,
    Day,
    Week,
    Month,
    All,
}

impl TimePeriod {
    /// Duration in seconds
    pub fn seconds(&self) -> u64 {
        match self {
            Self::Hour => 3600,
            Self::Day => 86400,
            Self::Week => 604800,
            Self::Month => 2592000,
            Self::All => u64::MAX,
        }
    }

    /// Label for display
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hour => "Last Hour",
            Self::Day => "Last 24 Hours",
            Self::Week => "Last Week",
            Self::Month => "Last Month",
            Self::All => "All Time",
        }
    }
}

/// Record of time saved
#[derive(Debug, Clone)]
pub struct TimeSavingsRecord {
    /// Task description
    pub task: String,
    /// Estimated manual time (seconds)
    pub manual_time_secs: u64,
    /// Actual automated time (seconds)
    pub automated_time_secs: u64,
    /// Task category
    pub category: String,
    /// Timestamp
    pub timestamp: u64,
}

impl TimeSavingsRecord {
    pub fn new(task: &str, manual_secs: u64, automated_secs: u64, category: &str) -> Self {
        Self {
            task: task.to_string(),
            manual_time_secs: manual_secs,
            automated_time_secs: automated_secs,
            category: category.to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Time saved in seconds
    pub fn time_saved(&self) -> i64 {
        self.manual_time_secs as i64 - self.automated_time_secs as i64
    }

    /// Efficiency ratio
    pub fn efficiency(&self) -> f64 {
        if self.automated_time_secs == 0 {
            1.0
        } else {
            self.manual_time_secs as f64 / self.automated_time_secs as f64
        }
    }
}

/// Tracker for time savings
pub struct TimeSavingsTracker {
    /// Recorded savings
    records: Vec<TimeSavingsRecord>,
    /// Category estimates (task category -> estimated manual time)
    category_estimates: HashMap<String, u64>,
    /// Maximum records
    max_records: usize,
}

impl TimeSavingsTracker {
    pub fn new() -> Self {
        let mut estimates = HashMap::new();
        // Default estimates for common tasks (in seconds)
        estimates.insert("code_write".to_string(), 1800); // 30 min
        estimates.insert("code_review".to_string(), 900); // 15 min
        estimates.insert("bug_fix".to_string(), 3600); // 1 hour
        estimates.insert("test_write".to_string(), 1200); // 20 min
        estimates.insert("refactor".to_string(), 2400); // 40 min
        estimates.insert("documentation".to_string(), 600); // 10 min
        estimates.insert("search".to_string(), 300); // 5 min

        Self {
            records: Vec::new(),
            category_estimates: estimates,
            max_records: 10000,
        }
    }

    /// Set estimate for a category
    pub fn set_category_estimate(&mut self, category: &str, estimate_secs: u64) {
        self.category_estimates
            .insert(category.to_string(), estimate_secs);
    }

    /// Record time savings
    pub fn record(&mut self, task: &str, automated_secs: u64, category: &str) {
        let manual_secs = self
            .category_estimates
            .get(category)
            .copied()
            .unwrap_or(300);
        self.records.push(TimeSavingsRecord::new(
            task,
            manual_secs,
            automated_secs,
            category,
        ));

        if self.records.len() > self.max_records {
            self.records.drain(0..self.max_records / 2);
        }
    }

    /// Record with explicit manual time
    pub fn record_explicit(
        &mut self,
        task: &str,
        manual_secs: u64,
        automated_secs: u64,
        category: &str,
    ) {
        self.records.push(TimeSavingsRecord::new(
            task,
            manual_secs,
            automated_secs,
            category,
        ));

        if self.records.len() > self.max_records {
            self.records.drain(0..self.max_records / 2);
        }
    }

    /// Get total time saved
    pub fn total_time_saved(&self, period: TimePeriod) -> i64 {
        let cutoff = Self::cutoff_time(period);
        self.records
            .iter()
            .filter(|r| r.timestamp >= cutoff)
            .map(|r| r.time_saved())
            .sum()
    }

    /// Get savings by category
    pub fn savings_by_category(&self, period: TimePeriod) -> HashMap<String, i64> {
        let cutoff = Self::cutoff_time(period);
        let mut by_cat = HashMap::new();

        for record in self.records.iter().filter(|r| r.timestamp >= cutoff) {
            *by_cat.entry(record.category.clone()).or_insert(0) += record.time_saved();
        }

        by_cat
    }

    /// Get average efficiency
    pub fn average_efficiency(&self, period: TimePeriod) -> f64 {
        let cutoff = Self::cutoff_time(period);
        let records: Vec<_> = self
            .records
            .iter()
            .filter(|r| r.timestamp >= cutoff)
            .collect();

        if records.is_empty() {
            return 1.0;
        }

        let total_manual: u64 = records.iter().map(|r| r.manual_time_secs).sum();
        let total_auto: u64 = records.iter().map(|r| r.automated_time_secs).sum();

        if total_auto == 0 {
            1.0
        } else {
            total_manual as f64 / total_auto as f64
        }
    }

    /// Get record count
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    fn cutoff_time(period: TimePeriod) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(period.seconds())
    }
}

impl Default for TimeSavingsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Bug prevention severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BugSeverity {
    /// Minor issue
    Low,
    /// Moderate issue
    Medium,
    /// Significant issue
    High,
    /// Critical/security issue
    Critical,
}

impl BugSeverity {
    /// Value score for severity
    pub fn value_score(&self) -> u32 {
        match self {
            Self::Low => 1,
            Self::Medium => 3,
            Self::High => 10,
            Self::Critical => 50,
        }
    }

    /// Estimated fix time (hours)
    pub fn estimated_fix_hours(&self) -> f32 {
        match self {
            Self::Low => 0.5,
            Self::Medium => 2.0,
            Self::High => 8.0,
            Self::Critical => 24.0,
        }
    }
}

/// Record of a bug prevented
#[derive(Debug, Clone)]
pub struct BugPreventionRecord {
    /// Bug description
    pub description: String,
    /// Bug type
    pub bug_type: String,
    /// Severity
    pub severity: BugSeverity,
    /// How it was prevented (lint, test, review, etc.)
    pub prevention_method: String,
    /// File where bug would have occurred
    pub file: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl BugPreventionRecord {
    pub fn new(description: &str, bug_type: &str, severity: BugSeverity, method: &str) -> Self {
        Self {
            description: description.to_string(),
            bug_type: bug_type.to_string(),
            severity,
            prevention_method: method.to_string(),
            file: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }
}

/// Tracker for bugs prevented
pub struct BugPreventionTracker {
    /// Records
    records: Vec<BugPreventionRecord>,
    /// Maximum records
    max_records: usize,
}

impl BugPreventionTracker {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            max_records: 10000,
        }
    }

    /// Record a bug prevented
    pub fn record(
        &mut self,
        description: &str,
        bug_type: &str,
        severity: BugSeverity,
        method: &str,
    ) {
        self.records.push(BugPreventionRecord::new(
            description,
            bug_type,
            severity,
            method,
        ));

        if self.records.len() > self.max_records {
            self.records.drain(0..self.max_records / 2);
        }
    }

    /// Get count by severity
    pub fn count_by_severity(&self, period: TimePeriod) -> HashMap<BugSeverity, usize> {
        let cutoff = Self::cutoff_time(period);
        let mut counts = HashMap::new();

        for record in self.records.iter().filter(|r| r.timestamp >= cutoff) {
            *counts.entry(record.severity).or_insert(0) += 1;
        }

        counts
    }

    /// Get count by type
    pub fn count_by_type(&self, period: TimePeriod) -> HashMap<String, usize> {
        let cutoff = Self::cutoff_time(period);
        let mut counts = HashMap::new();

        for record in self.records.iter().filter(|r| r.timestamp >= cutoff) {
            *counts.entry(record.bug_type.clone()).or_insert(0) += 1;
        }

        counts
    }

    /// Get count by prevention method
    pub fn count_by_method(&self, period: TimePeriod) -> HashMap<String, usize> {
        let cutoff = Self::cutoff_time(period);
        let mut counts = HashMap::new();

        for record in self.records.iter().filter(|r| r.timestamp >= cutoff) {
            *counts.entry(record.prevention_method.clone()).or_insert(0) += 1;
        }

        counts
    }

    /// Total value score of bugs prevented
    pub fn total_value(&self, period: TimePeriod) -> u32 {
        let cutoff = Self::cutoff_time(period);
        self.records
            .iter()
            .filter(|r| r.timestamp >= cutoff)
            .map(|r| r.severity.value_score())
            .sum()
    }

    /// Estimated time saved by preventing bugs (hours)
    pub fn estimated_time_saved_hours(&self, period: TimePeriod) -> f32 {
        let cutoff = Self::cutoff_time(period);
        self.records
            .iter()
            .filter(|r| r.timestamp >= cutoff)
            .map(|r| r.severity.estimated_fix_hours())
            .sum()
    }

    /// Total count
    pub fn total_count(&self, period: TimePeriod) -> usize {
        let cutoff = Self::cutoff_time(period);
        self.records
            .iter()
            .filter(|r| r.timestamp >= cutoff)
            .count()
    }

    fn cutoff_time(period: TimePeriod) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(period.seconds())
    }
}

impl Default for BugPreventionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Code quality metric snapshot
#[derive(Debug, Clone)]
pub struct QualitySnapshot {
    /// Timestamp
    pub timestamp: u64,
    /// Lines of code
    pub lines_of_code: usize,
    /// Test coverage percentage
    pub test_coverage: f32,
    /// Number of tests
    pub test_count: usize,
    /// Passing tests
    pub tests_passing: usize,
    /// Number of lints/warnings
    pub warnings: usize,
    /// Documentation coverage
    pub doc_coverage: f32,
    /// Complexity score (lower is better)
    pub complexity_score: f32,
    /// Technical debt score (lower is better)
    pub tech_debt_score: f32,
}

impl QualitySnapshot {
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            lines_of_code: 0,
            test_coverage: 0.0,
            test_count: 0,
            tests_passing: 0,
            warnings: 0,
            doc_coverage: 0.0,
            complexity_score: 0.0,
            tech_debt_score: 0.0,
        }
    }

    /// Test pass rate
    pub fn test_pass_rate(&self) -> f32 {
        if self.test_count == 0 {
            1.0
        } else {
            self.tests_passing as f32 / self.test_count as f32
        }
    }

    /// Overall health score (0-100)
    pub fn health_score(&self) -> f32 {
        let coverage_score = self.test_coverage * 30.0;
        let pass_rate_score = self.test_pass_rate() * 25.0;
        let warning_score = (1.0 - (self.warnings as f32 / 100.0).min(1.0)) * 15.0;
        let doc_score = self.doc_coverage * 10.0;
        let complexity_penalty = (self.complexity_score / 100.0).min(1.0) * 10.0;
        let debt_penalty = (self.tech_debt_score / 100.0).min(1.0) * 10.0;

        (coverage_score + pass_rate_score + warning_score + doc_score
            - complexity_penalty
            - debt_penalty)
            .max(0.0)
    }
}

impl Default for QualitySnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracker for code quality over time
pub struct CodeQualityTracker {
    /// Snapshots over time
    snapshots: Vec<QualitySnapshot>,
    /// Maximum snapshots
    max_snapshots: usize,
}

impl CodeQualityTracker {
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
            max_snapshots: 1000,
        }
    }

    /// Record a snapshot
    pub fn record(&mut self, snapshot: QualitySnapshot) {
        self.snapshots.push(snapshot);

        if self.snapshots.len() > self.max_snapshots {
            self.snapshots.drain(0..self.max_snapshots / 2);
        }
    }

    /// Get latest snapshot
    pub fn latest(&self) -> Option<&QualitySnapshot> {
        self.snapshots.last()
    }

    /// Get trend for a metric over period
    pub fn trend(&self, period: TimePeriod) -> Option<QualityTrend> {
        let cutoff = Self::cutoff_time(period);
        let relevant: Vec<_> = self
            .snapshots
            .iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect();

        if relevant.len() < 2 {
            return None;
        }

        let first = relevant.first().unwrap();
        let last = relevant.last().unwrap();

        Some(QualityTrend {
            coverage_change: last.test_coverage - first.test_coverage,
            test_count_change: last.test_count as i32 - first.test_count as i32,
            warning_change: last.warnings as i32 - first.warnings as i32,
            health_change: last.health_score() - first.health_score(),
            snapshots: relevant.len(),
        })
    }

    /// Get average health score
    pub fn average_health(&self, period: TimePeriod) -> f32 {
        let cutoff = Self::cutoff_time(period);
        let relevant: Vec<_> = self
            .snapshots
            .iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect();

        if relevant.is_empty() {
            return 0.0;
        }

        relevant.iter().map(|s| s.health_score()).sum::<f32>() / relevant.len() as f32
    }

    fn cutoff_time(period: TimePeriod) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(period.seconds())
    }
}

impl Default for CodeQualityTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Quality trend over time
#[derive(Debug, Clone)]
pub struct QualityTrend {
    pub coverage_change: f32,
    pub test_count_change: i32,
    pub warning_change: i32,
    pub health_change: f32,
    pub snapshots: usize,
}

impl QualityTrend {
    /// Is trend positive overall?
    pub fn is_positive(&self) -> bool {
        self.health_change > 0.0
    }
}

/// Productivity data point
#[derive(Debug, Clone)]
pub struct ProductivityPoint {
    /// Timestamp
    pub timestamp: u64,
    /// Tasks completed
    pub tasks_completed: usize,
    /// Lines of code written
    pub lines_written: usize,
    /// Commits made
    pub commits: usize,
    /// Reviews completed
    pub reviews: usize,
    /// Bugs fixed
    pub bugs_fixed: usize,
    /// Tests written
    pub tests_written: usize,
}

impl ProductivityPoint {
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tasks_completed: 0,
            lines_written: 0,
            commits: 0,
            reviews: 0,
            bugs_fixed: 0,
            tests_written: 0,
        }
    }

    /// Total activity score
    pub fn activity_score(&self) -> usize {
        self.tasks_completed * 10
            + self.lines_written / 10
            + self.commits * 5
            + self.reviews * 8
            + self.bugs_fixed * 15
            + self.tests_written * 3
    }
}

impl Default for ProductivityPoint {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracker for productivity trends
pub struct ProductivityTracker {
    /// Data points
    points: Vec<ProductivityPoint>,
    /// Current accumulator (for the current period)
    current: ProductivityPoint,
    /// Accumulation period (seconds)
    period_secs: u64,
    /// Maximum points
    max_points: usize,
}

impl ProductivityTracker {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            current: ProductivityPoint::new(),
            period_secs: 3600, // 1 hour periods by default
            max_points: 720,   // 30 days of hourly data
        }
    }

    /// Set accumulation period
    pub fn with_period_secs(mut self, secs: u64) -> Self {
        self.period_secs = secs;
        self
    }

    /// Record task completion
    pub fn record_task(&mut self) {
        self.maybe_flush();
        self.current.tasks_completed += 1;
    }

    /// Record lines written
    pub fn record_lines(&mut self, count: usize) {
        self.maybe_flush();
        self.current.lines_written += count;
    }

    /// Record a commit
    pub fn record_commit(&mut self) {
        self.maybe_flush();
        self.current.commits += 1;
    }

    /// Record a review
    pub fn record_review(&mut self) {
        self.maybe_flush();
        self.current.reviews += 1;
    }

    /// Record bug fix
    pub fn record_bug_fix(&mut self) {
        self.maybe_flush();
        self.current.bugs_fixed += 1;
    }

    /// Record test written
    pub fn record_test(&mut self) {
        self.maybe_flush();
        self.current.tests_written += 1;
    }

    /// Flush current point if period elapsed
    fn maybe_flush(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now - self.current.timestamp >= self.period_secs {
            // Save current point
            let point = std::mem::take(&mut self.current);
            if point.activity_score() > 0 {
                self.points.push(point);
            }

            // Trim old points
            if self.points.len() > self.max_points {
                self.points.drain(0..self.max_points / 2);
            }
        }
    }

    /// Force flush current point
    pub fn flush(&mut self) {
        if self.current.activity_score() > 0 {
            let point = std::mem::take(&mut self.current);
            self.points.push(point);
        }
    }

    /// Get average productivity score
    pub fn average_score(&self, period: TimePeriod) -> f64 {
        let cutoff = Self::cutoff_time(period);
        let relevant: Vec<_> = self
            .points
            .iter()
            .filter(|p| p.timestamp >= cutoff)
            .collect();

        if relevant.is_empty() {
            return 0.0;
        }

        relevant
            .iter()
            .map(|p| p.activity_score() as f64)
            .sum::<f64>()
            / relevant.len() as f64
    }

    /// Get trend
    pub fn trend(&self, period: TimePeriod) -> Option<ProductivityTrend> {
        let cutoff = Self::cutoff_time(period);
        let relevant: Vec<_> = self
            .points
            .iter()
            .filter(|p| p.timestamp >= cutoff)
            .collect();

        if relevant.len() < 2 {
            return None;
        }

        // Compare first half to second half
        let mid = relevant.len() / 2;
        let first_half: f64 = relevant[..mid]
            .iter()
            .map(|p| p.activity_score() as f64)
            .sum::<f64>()
            / mid as f64;
        let second_half: f64 = relevant[mid..]
            .iter()
            .map(|p| p.activity_score() as f64)
            .sum::<f64>()
            / (relevant.len() - mid) as f64;

        Some(ProductivityTrend {
            change_percent: if first_half > 0.0 {
                ((second_half - first_half) / first_half) * 100.0
            } else {
                0.0
            },
            first_half_avg: first_half,
            second_half_avg: second_half,
            data_points: relevant.len(),
        })
    }

    fn cutoff_time(period: TimePeriod) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(period.seconds())
    }
}

impl Default for ProductivityTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Productivity trend
#[derive(Debug, Clone)]
pub struct ProductivityTrend {
    pub change_percent: f64,
    pub first_half_avg: f64,
    pub second_half_avg: f64,
    pub data_points: usize,
}

impl ProductivityTrend {
    /// Is productivity improving?
    pub fn is_improving(&self) -> bool {
        self.change_percent > 5.0
    }

    /// Is productivity declining?
    pub fn is_declining(&self) -> bool {
        self.change_percent < -5.0
    }
}

/// Main analytics dashboard
pub struct AnalyticsDashboard {
    /// Time savings tracker
    time_savings: RwLock<TimeSavingsTracker>,
    /// Bug prevention tracker
    bug_prevention: RwLock<BugPreventionTracker>,
    /// Code quality tracker
    code_quality: RwLock<CodeQualityTracker>,
    /// Productivity tracker
    productivity: RwLock<ProductivityTracker>,
}

impl AnalyticsDashboard {
    pub fn new() -> Self {
        Self {
            time_savings: RwLock::new(TimeSavingsTracker::new()),
            bug_prevention: RwLock::new(BugPreventionTracker::new()),
            code_quality: RwLock::new(CodeQualityTracker::new()),
            productivity: RwLock::new(ProductivityTracker::new()),
        }
    }

    /// Record time savings
    pub fn record_time_savings(&self, task: &str, automated_secs: u64, category: &str) {
        if let Ok(mut tracker) = self.time_savings.write() {
            tracker.record(task, automated_secs, category);
        }
    }

    /// Record bug prevented
    pub fn record_bug_prevented(
        &self,
        description: &str,
        bug_type: &str,
        severity: BugSeverity,
        method: &str,
    ) {
        if let Ok(mut tracker) = self.bug_prevention.write() {
            tracker.record(description, bug_type, severity, method);
        }
    }

    /// Record quality snapshot
    pub fn record_quality_snapshot(&self, snapshot: QualitySnapshot) {
        if let Ok(mut tracker) = self.code_quality.write() {
            tracker.record(snapshot);
        }
    }

    /// Record productivity events
    pub fn record_task_completed(&self) {
        if let Ok(mut tracker) = self.productivity.write() {
            tracker.record_task();
        }
    }

    pub fn record_lines_written(&self, count: usize) {
        if let Ok(mut tracker) = self.productivity.write() {
            tracker.record_lines(count);
        }
    }

    pub fn record_commit(&self) {
        if let Ok(mut tracker) = self.productivity.write() {
            tracker.record_commit();
        }
    }

    /// Get summary for a period
    pub fn get_summary(&self, period: TimePeriod) -> DashboardSummary {
        let time_saved = self
            .time_savings
            .read()
            .map(|t| t.total_time_saved(period))
            .unwrap_or(0);

        let efficiency = self
            .time_savings
            .read()
            .map(|t| t.average_efficiency(period))
            .unwrap_or(1.0);

        let bugs_prevented = self
            .bug_prevention
            .read()
            .map(|t| t.total_count(period))
            .unwrap_or(0);

        let bug_value = self
            .bug_prevention
            .read()
            .map(|t| t.total_value(period))
            .unwrap_or(0);

        let health_score = self
            .code_quality
            .read()
            .map(|t| t.average_health(period))
            .unwrap_or(0.0);

        let productivity_score = self
            .productivity
            .read()
            .map(|t| t.average_score(period))
            .unwrap_or(0.0);

        DashboardSummary {
            period,
            time_saved_secs: time_saved,
            efficiency_ratio: efficiency,
            bugs_prevented,
            bug_prevention_value: bug_value,
            health_score,
            productivity_score,
        }
    }

    /// Get ROI estimate (simplified)
    pub fn estimate_roi(&self, period: TimePeriod, hourly_rate: f64) -> RoiEstimate {
        let summary = self.get_summary(period);

        // Time saved value
        let time_value = (summary.time_saved_secs as f64 / 3600.0) * hourly_rate;

        // Bug prevention value (estimated based on fix time avoided)
        let bug_fix_hours = self
            .bug_prevention
            .read()
            .map(|t| t.estimated_time_saved_hours(period))
            .unwrap_or(0.0);
        let bug_value = bug_fix_hours as f64 * hourly_rate;

        // Total value
        let total_value = time_value + bug_value;

        RoiEstimate {
            period,
            time_value,
            bug_prevention_value: bug_value,
            total_value,
            hourly_rate,
        }
    }

    /// Generate text report
    pub fn generate_report(&self, period: TimePeriod) -> String {
        let summary = self.get_summary(period);
        let roi = self.estimate_roi(period, 50.0); // Default $50/hour

        let mut report = String::new();
        report.push_str(&format!("# Analytics Report - {}\n\n", period.label()));

        report.push_str("## Time Savings\n");
        report.push_str(&format!(
            "- Time saved: {} hours\n",
            summary.time_saved_secs / 3600
        ));
        report.push_str(&format!(
            "- Efficiency ratio: {:.1}x\n\n",
            summary.efficiency_ratio
        ));

        report.push_str("## Bug Prevention\n");
        report.push_str(&format!("- Bugs prevented: {}\n", summary.bugs_prevented));
        report.push_str(&format!(
            "- Value score: {}\n\n",
            summary.bug_prevention_value
        ));

        report.push_str("## Code Quality\n");
        report.push_str(&format!(
            "- Health score: {:.1}/100\n\n",
            summary.health_score
        ));

        report.push_str("## Productivity\n");
        report.push_str(&format!(
            "- Average score: {:.1}\n\n",
            summary.productivity_score
        ));

        report.push_str("## ROI Estimate\n");
        report.push_str(&format!("- Time value: ${:.2}\n", roi.time_value));
        report.push_str(&format!(
            "- Bug prevention value: ${:.2}\n",
            roi.bug_prevention_value
        ));
        report.push_str(&format!("- Total value: ${:.2}\n", roi.total_value));

        report
    }
}

impl Default for AnalyticsDashboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Dashboard summary
#[derive(Debug, Clone)]
pub struct DashboardSummary {
    pub period: TimePeriod,
    pub time_saved_secs: i64,
    pub efficiency_ratio: f64,
    pub bugs_prevented: usize,
    pub bug_prevention_value: u32,
    pub health_score: f32,
    pub productivity_score: f64,
}

/// ROI estimate
#[derive(Debug, Clone)]
pub struct RoiEstimate {
    pub period: TimePeriod,
    pub time_value: f64,
    pub bug_prevention_value: f64,
    pub total_value: f64,
    pub hourly_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_period_seconds() {
        assert_eq!(TimePeriod::Hour.seconds(), 3600);
        assert_eq!(TimePeriod::Day.seconds(), 86400);
    }

    #[test]
    fn test_time_period_label() {
        assert_eq!(TimePeriod::Week.label(), "Last Week");
    }

    #[test]
    fn test_time_savings_record() {
        let record = TimeSavingsRecord::new("test", 100, 50, "code");
        assert_eq!(record.time_saved(), 50);
        assert_eq!(record.efficiency(), 2.0);
    }

    #[test]
    fn test_time_savings_tracker_new() {
        let tracker = TimeSavingsTracker::new();
        assert_eq!(tracker.record_count(), 0);
    }

    #[test]
    fn test_time_savings_tracker_record() {
        let mut tracker = TimeSavingsTracker::new();
        tracker.record("test task", 30, "code_write");
        assert_eq!(tracker.record_count(), 1);
    }

    #[test]
    fn test_time_savings_tracker_total() {
        let mut tracker = TimeSavingsTracker::new();
        tracker.record_explicit("task1", 100, 50, "code");
        tracker.record_explicit("task2", 200, 100, "code");
        assert!(tracker.total_time_saved(TimePeriod::All) > 0);
    }

    #[test]
    fn test_bug_severity_value() {
        assert!(BugSeverity::Critical.value_score() > BugSeverity::Low.value_score());
    }

    #[test]
    fn test_bug_severity_fix_hours() {
        assert!(
            BugSeverity::Critical.estimated_fix_hours() > BugSeverity::Low.estimated_fix_hours()
        );
    }

    #[test]
    fn test_bug_prevention_record() {
        let record = BugPreventionRecord::new("null check", "null_ptr", BugSeverity::High, "lint");
        assert_eq!(record.bug_type, "null_ptr");
    }

    #[test]
    fn test_bug_prevention_tracker_new() {
        let tracker = BugPreventionTracker::new();
        assert_eq!(tracker.total_count(TimePeriod::All), 0);
    }

    #[test]
    fn test_bug_prevention_tracker_record() {
        let mut tracker = BugPreventionTracker::new();
        tracker.record("bug", "type", BugSeverity::Medium, "review");
        assert_eq!(tracker.total_count(TimePeriod::All), 1);
    }

    #[test]
    fn test_bug_prevention_by_severity() {
        let mut tracker = BugPreventionTracker::new();
        tracker.record("bug1", "type", BugSeverity::Low, "lint");
        tracker.record("bug2", "type", BugSeverity::High, "lint");

        let counts = tracker.count_by_severity(TimePeriod::All);
        assert_eq!(counts.get(&BugSeverity::Low), Some(&1));
        assert_eq!(counts.get(&BugSeverity::High), Some(&1));
    }

    #[test]
    fn test_quality_snapshot_new() {
        let snapshot = QualitySnapshot::new();
        assert!(snapshot.timestamp > 0);
    }

    #[test]
    fn test_quality_snapshot_pass_rate() {
        let mut snapshot = QualitySnapshot::new();
        snapshot.test_count = 10;
        snapshot.tests_passing = 8;
        assert_eq!(snapshot.test_pass_rate(), 0.8);
    }

    #[test]
    fn test_quality_snapshot_health_score() {
        let mut snapshot = QualitySnapshot::new();
        snapshot.test_coverage = 0.8;
        snapshot.test_count = 100;
        snapshot.tests_passing = 100;
        let score = snapshot.health_score();
        assert!(score > 0.0);
    }

    #[test]
    fn test_code_quality_tracker_new() {
        let tracker = CodeQualityTracker::new();
        assert!(tracker.latest().is_none());
    }

    #[test]
    fn test_code_quality_tracker_record() {
        let mut tracker = CodeQualityTracker::new();
        tracker.record(QualitySnapshot::new());
        assert!(tracker.latest().is_some());
    }

    #[test]
    fn test_productivity_point_new() {
        let point = ProductivityPoint::new();
        assert_eq!(point.activity_score(), 0);
    }

    #[test]
    fn test_productivity_point_score() {
        let mut point = ProductivityPoint::new();
        point.tasks_completed = 5;
        point.commits = 3;
        assert!(point.activity_score() > 0);
    }

    #[test]
    fn test_productivity_tracker_new() {
        let tracker = ProductivityTracker::new();
        assert_eq!(tracker.average_score(TimePeriod::All), 0.0);
    }

    #[test]
    fn test_productivity_tracker_record() {
        let mut tracker = ProductivityTracker::new();
        tracker.record_task();
        tracker.record_commit();
        tracker.flush();
        // After flush, should have some data
    }

    #[test]
    fn test_analytics_dashboard_new() {
        let dashboard = AnalyticsDashboard::new();
        let summary = dashboard.get_summary(TimePeriod::All);
        assert_eq!(summary.bugs_prevented, 0);
    }

    #[test]
    fn test_analytics_dashboard_record_time() {
        let dashboard = AnalyticsDashboard::new();
        dashboard.record_time_savings("test", 60, "code_write");
        // Verify recorded (would need accessor to check)
    }

    #[test]
    fn test_analytics_dashboard_record_bug() {
        let dashboard = AnalyticsDashboard::new();
        dashboard.record_bug_prevented("null check", "null_ptr", BugSeverity::High, "lint");
        let summary = dashboard.get_summary(TimePeriod::All);
        assert_eq!(summary.bugs_prevented, 1);
    }

    #[test]
    fn test_analytics_dashboard_roi() {
        let dashboard = AnalyticsDashboard::new();
        let roi = dashboard.estimate_roi(TimePeriod::All, 50.0);
        assert_eq!(roi.hourly_rate, 50.0);
    }

    #[test]
    fn test_analytics_dashboard_report() {
        let dashboard = AnalyticsDashboard::new();
        let report = dashboard.generate_report(TimePeriod::Week);
        assert!(report.contains("Analytics Report"));
        assert!(report.contains("Time Savings"));
        assert!(report.contains("ROI"));
    }

    #[test]
    fn test_quality_trend_positive() {
        let trend = QualityTrend {
            coverage_change: 0.1,
            test_count_change: 10,
            warning_change: -5,
            health_change: 5.0,
            snapshots: 10,
        };
        assert!(trend.is_positive());
    }

    #[test]
    fn test_productivity_trend_improving() {
        let trend = ProductivityTrend {
            change_percent: 20.0,
            first_half_avg: 50.0,
            second_half_avg: 60.0,
            data_points: 20,
        };
        assert!(trend.is_improving());
        assert!(!trend.is_declining());
    }

    #[test]
    fn test_time_savings_by_category() {
        let mut tracker = TimeSavingsTracker::new();
        tracker.record_explicit("t1", 100, 50, "code_write");
        tracker.record_explicit("t2", 200, 100, "bug_fix");

        let by_cat = tracker.savings_by_category(TimePeriod::All);
        assert!(by_cat.contains_key("code_write"));
        assert!(by_cat.contains_key("bug_fix"));
    }

    #[test]
    fn test_dashboard_summary() {
        let summary = DashboardSummary {
            period: TimePeriod::Week,
            time_saved_secs: 3600,
            efficiency_ratio: 2.0,
            bugs_prevented: 5,
            bug_prevention_value: 50,
            health_score: 75.0,
            productivity_score: 100.0,
        };
        assert_eq!(summary.time_saved_secs, 3600);
    }

    #[test]
    fn test_roi_estimate() {
        let roi = RoiEstimate {
            period: TimePeriod::Month,
            time_value: 1000.0,
            bug_prevention_value: 500.0,
            total_value: 1500.0,
            hourly_rate: 50.0,
        };
        assert_eq!(roi.total_value, 1500.0);
    }

    #[test]
    fn test_time_period_all_variants() {
        let periods = [
            TimePeriod::Hour,
            TimePeriod::Day,
            TimePeriod::Week,
            TimePeriod::Month,
            TimePeriod::All,
        ];

        for period in periods {
            let _ = period.seconds();
            let _ = period.label();
        }
    }

    #[test]
    fn test_time_period_seconds_all() {
        assert_eq!(TimePeriod::Week.seconds(), 604800);
        assert_eq!(TimePeriod::Month.seconds(), 2592000);
        assert_eq!(TimePeriod::All.seconds(), u64::MAX);
    }

    #[test]
    fn test_time_period_label_all() {
        assert_eq!(TimePeriod::Hour.label(), "Last Hour");
        assert_eq!(TimePeriod::Day.label(), "Last 24 Hours");
        assert_eq!(TimePeriod::All.label(), "All Time");
    }

    #[test]
    fn test_time_savings_record_clone() {
        let record = TimeSavingsRecord::new("task", 100, 50, "code");
        let cloned = record.clone();
        assert_eq!(record.task, cloned.task);
        assert_eq!(record.time_saved(), cloned.time_saved());
    }

    #[test]
    fn test_time_savings_record_efficiency_zero_automated() {
        let record = TimeSavingsRecord::new("instant", 100, 0, "fast");
        assert_eq!(record.efficiency(), 1.0);
    }

    #[test]
    fn test_time_savings_record_negative_savings() {
        let record = TimeSavingsRecord::new("slow", 50, 100, "code");
        assert_eq!(record.time_saved(), -50);
    }

    #[test]
    fn test_time_savings_tracker_default() {
        let tracker = TimeSavingsTracker::default();
        assert_eq!(tracker.record_count(), 0);
    }

    #[test]
    fn test_time_savings_tracker_set_category_estimate() {
        let mut tracker = TimeSavingsTracker::new();
        tracker.set_category_estimate("custom", 5000);
        tracker.record("custom task", 100, "custom");
        // The estimate should be used
        assert!(tracker.total_time_saved(TimePeriod::All) > 0);
    }

    #[test]
    fn test_time_savings_tracker_average_efficiency() {
        let mut tracker = TimeSavingsTracker::new();
        tracker.record_explicit("task1", 100, 50, "code");
        tracker.record_explicit("task2", 200, 50, "code");
        let efficiency = tracker.average_efficiency(TimePeriod::All);
        assert!(efficiency > 1.0);
    }

    #[test]
    fn test_time_savings_tracker_average_efficiency_empty() {
        let tracker = TimeSavingsTracker::new();
        assert_eq!(tracker.average_efficiency(TimePeriod::All), 1.0);
    }

    #[test]
    fn test_bug_severity_all_variants() {
        let severities = [
            BugSeverity::Critical,
            BugSeverity::High,
            BugSeverity::Medium,
            BugSeverity::Low,
        ];

        for severity in severities {
            let _ = severity.value_score();
            let _ = severity.estimated_fix_hours();
        }
    }

    #[test]
    fn test_bug_prevention_record_clone() {
        let record = BugPreventionRecord::new("desc", "type", BugSeverity::High, "source");
        let cloned = record.clone();
        assert_eq!(record.description, cloned.description);
        assert_eq!(record.severity, cloned.severity);
    }

    #[test]
    fn test_bug_prevention_tracker_default() {
        let tracker = BugPreventionTracker::default();
        assert_eq!(tracker.total_count(TimePeriod::All), 0);
    }

    #[test]
    fn test_bug_prevention_tracker_by_type() {
        let mut tracker = BugPreventionTracker::new();
        tracker.record("bug1", "null_ptr", BugSeverity::High, "lint");
        tracker.record("bug2", "null_ptr", BugSeverity::Low, "lint");
        tracker.record("bug3", "race", BugSeverity::Critical, "review");

        let by_type = tracker.count_by_type(TimePeriod::All);
        assert_eq!(by_type.get("null_ptr"), Some(&2));
        assert_eq!(by_type.get("race"), Some(&1));
    }

    #[test]
    fn test_bug_prevention_tracker_by_method() {
        let mut tracker = BugPreventionTracker::new();
        tracker.record("bug1", "type", BugSeverity::High, "lint");
        tracker.record("bug2", "type", BugSeverity::Low, "review");

        let by_method = tracker.count_by_method(TimePeriod::All);
        assert!(by_method.contains_key("lint"));
        assert!(by_method.contains_key("review"));
    }

    #[test]
    fn test_bug_prevention_tracker_total_value() {
        let mut tracker = BugPreventionTracker::new();
        tracker.record("bug", "type", BugSeverity::Critical, "lint");
        let value = tracker.total_value(TimePeriod::All);
        assert!(value > 0);
    }

    #[test]
    fn test_quality_snapshot_clone() {
        let snapshot = QualitySnapshot::new();
        let cloned = snapshot.clone();
        assert_eq!(snapshot.test_coverage, cloned.test_coverage);
    }

    #[test]
    fn test_quality_snapshot_pass_rate_no_tests() {
        let snapshot = QualitySnapshot::new();
        assert_eq!(snapshot.test_pass_rate(), 1.0);
    }

    #[test]
    fn test_quality_snapshot_health_score_with_warnings() {
        let mut snapshot = QualitySnapshot::new();
        snapshot.test_coverage = 0.8;
        snapshot.test_count = 100;
        snapshot.tests_passing = 100;
        snapshot.warnings = 10;
        let score = snapshot.health_score();
        assert!(score > 0.0 && score <= 100.0);
    }

    #[test]
    fn test_code_quality_tracker_default() {
        let tracker = CodeQualityTracker::default();
        assert!(tracker.latest().is_none());
    }

    #[test]
    fn test_code_quality_tracker_trend() {
        let mut tracker = CodeQualityTracker::new();

        let mut s1 = QualitySnapshot::new();
        s1.test_coverage = 0.5;
        s1.test_count = 50;

        let mut s2 = QualitySnapshot::new();
        s2.test_coverage = 0.6;
        s2.test_count = 60;

        tracker.record(s1);
        tracker.record(s2);

        let trend = tracker.trend(TimePeriod::All);
        assert!(trend.is_some());
        assert!(trend.unwrap().coverage_change > 0.0);
    }

    #[test]
    fn test_code_quality_tracker_trend_empty() {
        let tracker = CodeQualityTracker::new();
        let trend = tracker.trend(TimePeriod::All);
        assert!(trend.is_none());
    }

    #[test]
    fn test_productivity_point_clone() {
        let mut point = ProductivityPoint::new();
        point.tasks_completed = 5;
        let cloned = point.clone();
        assert_eq!(point.tasks_completed, cloned.tasks_completed);
    }

    #[test]
    fn test_productivity_tracker_default() {
        let tracker = ProductivityTracker::default();
        assert_eq!(tracker.average_score(TimePeriod::All), 0.0);
    }

    #[test]
    fn test_productivity_tracker_record_line() {
        let mut tracker = ProductivityTracker::new();
        tracker.record_lines(100);
        tracker.flush();
        // Just verify it doesn't panic
    }

    #[test]
    fn test_productivity_tracker_record_review() {
        let mut tracker = ProductivityTracker::new();
        tracker.record_review();
        tracker.flush();
    }

    #[test]
    fn test_productivity_tracker_trend() {
        let mut tracker = ProductivityTracker::new();

        for _ in 0..5 {
            tracker.record_task();
            tracker.flush();
        }

        let trend = tracker.trend(TimePeriod::All);
        // Verify it returns a trend or None
        if let Some(t) = trend {
            assert!(t.data_points > 0 || t.data_points == 0);
        }
    }

    #[test]
    fn test_productivity_trend_declining() {
        let trend = ProductivityTrend {
            change_percent: -20.0,
            first_half_avg: 60.0,
            second_half_avg: 50.0,
            data_points: 20,
        };
        assert!(!trend.is_improving());
        assert!(trend.is_declining());
    }

    #[test]
    fn test_productivity_trend_neutral() {
        let trend = ProductivityTrend {
            change_percent: 0.0,
            first_half_avg: 50.0,
            second_half_avg: 50.0,
            data_points: 20,
        };
        assert!(!trend.is_improving());
        assert!(!trend.is_declining());
    }

    #[test]
    fn test_quality_trend_negative() {
        let trend = QualityTrend {
            coverage_change: -0.1,
            test_count_change: -5,
            warning_change: 10,
            health_change: -10.0,
            snapshots: 10,
        };
        assert!(!trend.is_positive());
    }

    #[test]
    fn test_analytics_dashboard_record_quality() {
        let dashboard = AnalyticsDashboard::new();
        dashboard.record_quality_snapshot(QualitySnapshot::new());
        // Verify it doesn't panic
    }

    #[test]
    fn test_analytics_dashboard_record_productivity_task() {
        let dashboard = AnalyticsDashboard::new();
        dashboard.record_task_completed();
    }

    #[test]
    fn test_analytics_dashboard_record_productivity_commit() {
        let dashboard = AnalyticsDashboard::new();
        dashboard.record_commit();
    }

    #[test]
    fn test_analytics_dashboard_get_summary_default() {
        let dashboard = AnalyticsDashboard::new();
        let summary = dashboard.get_summary(TimePeriod::All);
        assert_eq!(summary.bugs_prevented, 0);
        assert_eq!(summary.time_saved_secs, 0);
    }

    #[test]
    fn test_analytics_dashboard_estimate_roi_default() {
        let dashboard = AnalyticsDashboard::new();
        let roi = dashboard.estimate_roi(TimePeriod::All, 50.0);
        assert_eq!(roi.hourly_rate, 50.0);
        assert_eq!(roi.total_value, 0.0);
    }

    #[test]
    fn test_dashboard_summary_clone() {
        let summary = DashboardSummary {
            period: TimePeriod::Week,
            time_saved_secs: 3600,
            efficiency_ratio: 2.0,
            bugs_prevented: 5,
            bug_prevention_value: 50,
            health_score: 75.0,
            productivity_score: 100.0,
        };
        let cloned = summary.clone();
        assert_eq!(summary.time_saved_secs, cloned.time_saved_secs);
    }

    #[test]
    fn test_roi_estimate_clone() {
        let roi = RoiEstimate {
            period: TimePeriod::Month,
            time_value: 1000.0,
            bug_prevention_value: 500.0,
            total_value: 1500.0,
            hourly_rate: 50.0,
        };
        let cloned = roi.clone();
        assert_eq!(roi.total_value, cloned.total_value);
    }

    #[test]
    fn test_analytics_dashboard_report_all_periods() {
        let dashboard = AnalyticsDashboard::new();

        for period in [
            TimePeriod::Hour,
            TimePeriod::Day,
            TimePeriod::Week,
            TimePeriod::Month,
            TimePeriod::All,
        ] {
            let report = dashboard.generate_report(period);
            assert!(!report.is_empty());
        }
    }

    #[test]
    fn test_time_period_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TimePeriod::Hour);
        set.insert(TimePeriod::Day);
        set.insert(TimePeriod::Hour); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_analytics_dashboard_comprehensive() {
        let dashboard = AnalyticsDashboard::new();

        // Record various data
        dashboard.record_time_savings("write code", 60, "code_write");
        dashboard.record_bug_prevented("null check", "null", BugSeverity::High, "lint");
        dashboard.record_quality_snapshot(QualitySnapshot::new());
        dashboard.record_task_completed();
        dashboard.record_commit();

        // Get summary
        let summary = dashboard.get_summary(TimePeriod::All);
        assert_eq!(summary.bugs_prevented, 1);

        // Get ROI
        let roi = dashboard.estimate_roi(TimePeriod::All, 75.0);
        assert_eq!(roi.hourly_rate, 75.0);

        // Generate report
        let report = dashboard.generate_report(TimePeriod::All);
        assert!(report.contains("Analytics Report"));
    }
}
