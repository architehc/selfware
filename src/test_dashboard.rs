//! Test Dashboard
//!
//! Testing UI with:
//! - Test explorer tree view
//! - Watch mode with debouncing
//! - Coverage tracking
//! - Failing test isolation
//! - Progress visualization

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Test status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TestStatus {
    #[default]
    Pending,
    Running,
    Passed,
    Failed,
    Ignored,
    Skipped,
}

impl TestStatus {
    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            TestStatus::Pending => "○",
            TestStatus::Running => "◐",
            TestStatus::Passed => "✓",
            TestStatus::Failed => "✗",
            TestStatus::Ignored => "⊘",
            TestStatus::Skipped => "⊝",
        }
    }

    /// Color code
    pub fn color(&self) -> &'static str {
        match self {
            TestStatus::Pending => "\x1b[90m", // Gray
            TestStatus::Running => "\x1b[33m", // Yellow
            TestStatus::Passed => "\x1b[32m",  // Green
            TestStatus::Failed => "\x1b[31m",  // Red
            TestStatus::Ignored => "\x1b[35m", // Magenta
            TestStatus::Skipped => "\x1b[90m", // Gray
        }
    }

    /// Is this a final state?
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            TestStatus::Passed | TestStatus::Failed | TestStatus::Ignored | TestStatus::Skipped
        )
    }
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            TestStatus::Pending => "Pending",
            TestStatus::Running => "Running",
            TestStatus::Passed => "Passed",
            TestStatus::Failed => "Failed",
            TestStatus::Ignored => "Ignored",
            TestStatus::Skipped => "Skipped",
        };
        write!(f, "{}", name)
    }
}

/// A single test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Test {
    /// Full test name (module::submodule::test_name)
    pub name: String,
    /// Module path
    pub module: String,
    /// Test function name
    pub function: String,
    /// Source file
    pub file: Option<PathBuf>,
    /// Line number
    pub line: Option<usize>,
    /// Current status
    pub status: TestStatus,
    /// Duration of last run
    pub duration: Option<Duration>,
    /// Failure message if failed
    pub failure_message: Option<String>,
    /// Failure location
    pub failure_location: Option<String>,
    /// Is this test #[ignore]d?
    pub ignored: bool,
    /// Tags/attributes
    pub tags: Vec<String>,
}

impl Test {
    /// Create a new test
    pub fn new(name: String) -> Self {
        let parts: Vec<&str> = name.rsplitn(2, "::").collect();
        let (function, module) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            (name.clone(), String::new())
        };

        Self {
            name,
            module,
            function,
            file: None,
            line: None,
            status: TestStatus::Pending,
            duration: None,
            failure_message: None,
            failure_location: None,
            ignored: false,
            tags: Vec::new(),
        }
    }

    /// Set file and line
    pub fn with_location(mut self, file: PathBuf, line: usize) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self
    }

    /// Mark as running
    pub fn start(&mut self) {
        self.status = TestStatus::Running;
    }

    /// Mark as passed
    pub fn pass(&mut self, duration: Duration) {
        self.status = TestStatus::Passed;
        self.duration = Some(duration);
        self.failure_message = None;
    }

    /// Mark as failed
    pub fn fail(&mut self, duration: Duration, message: String) {
        self.status = TestStatus::Failed;
        self.duration = Some(duration);
        self.failure_message = Some(message);
    }

    /// Mark as ignored
    pub fn ignore(&mut self) {
        self.status = TestStatus::Ignored;
        self.ignored = true;
    }

    /// Display formatted
    pub fn display(&self) -> String {
        let duration = self
            .duration
            .map(|d| format!(" ({:.2}s)", d.num_milliseconds() as f64 / 1000.0))
            .unwrap_or_default();

        format!(
            "{}{} {}{}",
            self.status.color(),
            self.status.icon(),
            self.name,
            duration
        )
    }

    /// Short display (just status and function name)
    pub fn short_display(&self) -> String {
        format!("{} {}", self.status.icon(), self.function)
    }
}

/// Test suite (collection of tests)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite name (crate/module name)
    pub name: String,
    /// All tests in this suite
    pub tests: Vec<Test>,
    /// Child suites (submodules)
    pub children: Vec<TestSuite>,
    /// Total test count (including children)
    pub total: usize,
    /// Passed count
    pub passed: usize,
    /// Failed count
    pub failed: usize,
    /// Ignored count
    pub ignored: usize,
}

impl TestSuite {
    /// Create new suite
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    /// Add a test to the suite
    pub fn add_test(&mut self, test: Test) {
        self.tests.push(test);
        self.update_counts();
    }

    /// Add a child suite
    pub fn add_child(&mut self, suite: TestSuite) {
        self.children.push(suite);
        self.update_counts();
    }

    /// Update counts from tests and children
    pub fn update_counts(&mut self) {
        let mut total = self.tests.len();
        let mut passed = 0;
        let mut failed = 0;
        let mut ignored = 0;

        for test in &self.tests {
            match test.status {
                TestStatus::Passed => passed += 1,
                TestStatus::Failed => failed += 1,
                TestStatus::Ignored | TestStatus::Skipped => ignored += 1,
                _ => {}
            }
        }

        for child in &self.children {
            total += child.total;
            passed += child.passed;
            failed += child.failed;
            ignored += child.ignored;
        }

        self.total = total;
        self.passed = passed;
        self.failed = failed;
        self.ignored = ignored;
    }

    /// Get all tests (flattened)
    pub fn all_tests(&self) -> Vec<&Test> {
        let mut all: Vec<&Test> = self.tests.iter().collect();
        for child in &self.children {
            all.extend(child.all_tests());
        }
        all
    }

    /// Get failed tests
    pub fn failed_tests(&self) -> Vec<&Test> {
        self.all_tests()
            .into_iter()
            .filter(|t| t.status == TestStatus::Failed)
            .collect()
    }

    /// Get passed tests
    pub fn passed_tests(&self) -> Vec<&Test> {
        self.all_tests()
            .into_iter()
            .filter(|t| t.status == TestStatus::Passed)
            .collect()
    }

    /// Is all passed?
    pub fn is_passing(&self) -> bool {
        self.failed == 0
    }

    /// Progress percentage
    pub fn progress(&self) -> f32 {
        if self.total == 0 {
            return 100.0;
        }
        ((self.passed + self.failed + self.ignored) as f32 / self.total as f32) * 100.0
    }

    /// Display summary
    pub fn summary(&self) -> String {
        format!(
            "{}: {} total, {} passed, {} failed, {} ignored",
            self.name, self.total, self.passed, self.failed, self.ignored
        )
    }
}

/// Test run result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRun {
    /// Run ID
    pub id: String,
    /// Start time
    pub started_at: DateTime<Utc>,
    /// End time
    pub ended_at: Option<DateTime<Utc>>,
    /// Test results
    pub results: Vec<Test>,
    /// Overall status
    pub status: RunStatus,
    /// Total duration
    pub duration: Option<Duration>,
}

/// Run status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RunStatus {
    #[default]
    Running,
    Passed,
    Failed,
    Cancelled,
}

impl TestRun {
    /// Create new run
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            ended_at: None,
            results: Vec::new(),
            status: RunStatus::Running,
            duration: None,
        }
    }

    /// Add test result
    pub fn add_result(&mut self, test: Test) {
        self.results.push(test);
    }

    /// Complete the run
    pub fn complete(&mut self) {
        self.ended_at = Some(Utc::now());
        self.duration = Some(Utc::now() - self.started_at);

        // Determine overall status
        let has_failures = self.results.iter().any(|t| t.status == TestStatus::Failed);
        self.status = if has_failures {
            RunStatus::Failed
        } else {
            RunStatus::Passed
        };
    }

    /// Cancel the run
    pub fn cancel(&mut self) {
        self.ended_at = Some(Utc::now());
        self.duration = Some(Utc::now() - self.started_at);
        self.status = RunStatus::Cancelled;
    }

    /// Get summary
    pub fn summary(&self) -> TestRunSummary {
        TestRunSummary {
            total: self.results.len(),
            passed: self
                .results
                .iter()
                .filter(|t| t.status == TestStatus::Passed)
                .count(),
            failed: self
                .results
                .iter()
                .filter(|t| t.status == TestStatus::Failed)
                .count(),
            ignored: self
                .results
                .iter()
                .filter(|t| t.status == TestStatus::Ignored)
                .count(),
            duration: self.duration,
        }
    }
}

impl Default for TestRun {
    fn default() -> Self {
        Self::new()
    }
}

/// Test run summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub duration: Option<Duration>,
}

impl TestRunSummary {
    /// Success rate as percentage
    pub fn success_rate(&self) -> f32 {
        if self.total == 0 {
            return 100.0;
        }
        (self.passed as f32 / self.total as f32) * 100.0
    }

    /// Display
    pub fn display(&self) -> String {
        let duration = self
            .duration
            .map(|d| format!(" in {:.2}s", d.num_milliseconds() as f64 / 1000.0))
            .unwrap_or_default();

        format!(
            "{} passed, {} failed, {} ignored ({:.1}% pass rate){}",
            self.passed,
            self.failed,
            self.ignored,
            self.success_rate(),
            duration
        )
    }
}

/// Test explorer tree node
#[derive(Debug, Clone)]
pub struct TestTreeNode {
    /// Node name
    pub name: String,
    /// Full path
    pub path: String,
    /// Is this a test (leaf) or module (branch)?
    pub is_test: bool,
    /// Status if test
    pub status: Option<TestStatus>,
    /// Children
    pub children: Vec<TestTreeNode>,
    /// Is expanded in UI?
    pub expanded: bool,
}

impl Default for TestTreeNode {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: String::new(),
            is_test: false,
            status: None,
            children: Vec::new(),
            expanded: true,
        }
    }
}

impl TestTreeNode {
    /// Create module node
    pub fn module(name: String, path: String) -> Self {
        Self {
            name,
            path,
            is_test: false,
            status: None,
            children: Vec::new(),
            expanded: true,
        }
    }

    /// Create test node
    pub fn test(name: String, path: String, status: TestStatus) -> Self {
        Self {
            name,
            path,
            is_test: true,
            status: Some(status),
            children: Vec::new(),
            expanded: false,
        }
    }

    /// Add child node
    pub fn add_child(&mut self, child: TestTreeNode) {
        self.children.push(child);
    }

    /// Find or create a path in the tree
    pub fn find_or_create(&mut self, parts: &[&str]) -> &mut TestTreeNode {
        if parts.is_empty() {
            return self;
        }

        let name = parts[0];
        let path = if self.path.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.path, name)
        };

        // Find existing child
        let idx = self.children.iter().position(|c| c.name == name);

        if let Some(idx) = idx {
            self.children[idx].find_or_create(&parts[1..])
        } else {
            // Create new child
            let child = TestTreeNode::module(name.to_string(), path);
            self.children.push(child);
            let last = self.children.len() - 1;
            self.children[last].find_or_create(&parts[1..])
        }
    }

    /// Toggle expansion
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Count tests in this node and children
    pub fn test_count(&self) -> usize {
        let own = if self.is_test { 1 } else { 0 };
        own + self.children.iter().map(|c| c.test_count()).sum::<usize>()
    }

    /// Count failed tests
    pub fn failed_count(&self) -> usize {
        let own = if self.status == Some(TestStatus::Failed) {
            1
        } else {
            0
        };
        own + self
            .children
            .iter()
            .map(|c| c.failed_count())
            .sum::<usize>()
    }

    /// Aggregate status for modules
    pub fn aggregate_status(&self) -> TestStatus {
        if self.is_test {
            return self.status.unwrap_or(TestStatus::Pending);
        }

        let mut has_failed = false;
        let mut has_running = false;
        let mut all_passed = true;

        for child in &self.children {
            let status = child.aggregate_status();
            match status {
                TestStatus::Failed => has_failed = true,
                TestStatus::Running => has_running = true,
                TestStatus::Passed => {}
                _ => all_passed = false,
            }
        }

        if has_failed {
            TestStatus::Failed
        } else if has_running {
            TestStatus::Running
        } else if all_passed && !self.children.is_empty() {
            TestStatus::Passed
        } else {
            TestStatus::Pending
        }
    }
}

/// Test explorer for navigating tests
#[derive(Debug, Default)]
pub struct TestExplorer {
    /// Root of test tree
    root: TestTreeNode,
    /// Currently selected path
    selected: Option<String>,
    /// Filter pattern
    filter: Option<String>,
    /// Show only failed tests
    show_failed_only: bool,
}

impl TestExplorer {
    /// Create new explorer
    pub fn new() -> Self {
        Self {
            root: TestTreeNode::module(String::new(), String::new()),
            selected: None,
            filter: None,
            show_failed_only: false,
        }
    }

    /// Build tree from tests
    pub fn build_tree(&mut self, tests: &[Test]) {
        self.root = TestTreeNode::module(String::new(), String::new());

        for test in tests {
            let parts: Vec<&str> = test.name.split("::").collect();
            if parts.len() > 1 {
                // Navigate to parent module
                let parent = self.root.find_or_create(&parts[..parts.len() - 1]);
                // Add test as leaf
                parent.add_child(TestTreeNode::test(
                    test.function.clone(),
                    test.name.clone(),
                    test.status,
                ));
            } else {
                // Top-level test
                self.root.add_child(TestTreeNode::test(
                    test.function.clone(),
                    test.name.clone(),
                    test.status,
                ));
            }
        }
    }

    /// Get visible nodes (respecting expansion and filters)
    pub fn visible_nodes(&self) -> Vec<(usize, &TestTreeNode)> {
        let mut result = Vec::new();
        self.collect_visible(&self.root, 0, &mut result);
        result
    }

    fn collect_visible<'a>(
        &'a self,
        node: &'a TestTreeNode,
        depth: usize,
        result: &mut Vec<(usize, &'a TestTreeNode)>,
    ) {
        // Skip root
        if !node.path.is_empty() {
            // Apply filter
            if let Some(filter) = &self.filter {
                if !node.path.to_lowercase().contains(&filter.to_lowercase()) {
                    // Check if any child matches
                    if !node
                        .children
                        .iter()
                        .any(|c| c.path.to_lowercase().contains(&filter.to_lowercase()))
                    {
                        return;
                    }
                }
            }

            // Apply failed-only filter
            if self.show_failed_only && node.failed_count() == 0 {
                return;
            }

            result.push((depth, node));
        }

        if node.expanded || node.path.is_empty() {
            for child in &node.children {
                self.collect_visible(child, depth + 1, result);
            }
        }
    }

    /// Set filter
    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;
    }

    /// Toggle failed-only mode
    pub fn toggle_failed_only(&mut self) {
        self.show_failed_only = !self.show_failed_only;
    }

    /// Get selected test
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Select a test
    pub fn select(&mut self, path: String) {
        self.selected = Some(path);
    }
}

/// Watch mode for continuous testing
#[derive(Debug)]
pub struct WatchMode {
    /// Debounce duration
    debounce: Duration,
    /// Last trigger time
    last_trigger: Option<DateTime<Utc>>,
    /// Pending changes
    pending_changes: HashSet<PathBuf>,
    /// Test patterns to run
    test_patterns: Vec<String>,
    /// Is watch mode active?
    active: bool,
    /// Run on save
    run_on_save: bool,
}

impl Default for WatchMode {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchMode {
    /// Create new watch mode
    pub fn new() -> Self {
        Self {
            debounce: Duration::milliseconds(500),
            last_trigger: None,
            pending_changes: HashSet::new(),
            test_patterns: Vec::new(),
            active: false,
            run_on_save: true,
        }
    }

    /// Set debounce duration
    pub fn with_debounce(mut self, ms: i64) -> Self {
        self.debounce = Duration::milliseconds(ms);
        self
    }

    /// Start watch mode
    pub fn start(&mut self) {
        self.active = true;
    }

    /// Stop watch mode
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Is active?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Record a file change
    pub fn file_changed(&mut self, path: PathBuf) {
        if self.active {
            self.pending_changes.insert(path);
        }
    }

    /// Check if we should run tests (debounce check)
    pub fn should_run(&mut self) -> bool {
        if !self.active || self.pending_changes.is_empty() {
            return false;
        }

        let now = Utc::now();
        if let Some(last) = self.last_trigger {
            if now - last < self.debounce {
                return false;
            }
        }

        self.last_trigger = Some(now);
        true
    }

    /// Get and clear pending changes
    pub fn take_changes(&mut self) -> HashSet<PathBuf> {
        std::mem::take(&mut self.pending_changes)
    }

    /// Add test pattern to run
    pub fn add_pattern(&mut self, pattern: String) {
        self.test_patterns.push(pattern);
    }

    /// Get patterns
    pub fn patterns(&self) -> &[String] {
        &self.test_patterns
    }

    /// Clear patterns
    pub fn clear_patterns(&mut self) {
        self.test_patterns.clear();
    }

    /// Toggle run on save
    pub fn toggle_run_on_save(&mut self) {
        self.run_on_save = !self.run_on_save;
    }
}

/// Coverage data for a file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileCoverage {
    /// File path
    pub path: PathBuf,
    /// Lines covered
    pub covered_lines: HashSet<usize>,
    /// Lines not covered
    pub uncovered_lines: HashSet<usize>,
    /// Lines that are executable
    pub executable_lines: HashSet<usize>,
    /// Branch coverage data
    pub branches: Vec<BranchCoverage>,
}

impl FileCoverage {
    /// Create new coverage for a file
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            ..Default::default()
        }
    }

    /// Add covered line
    pub fn cover_line(&mut self, line: usize) {
        self.covered_lines.insert(line);
        self.executable_lines.insert(line);
        self.uncovered_lines.remove(&line);
    }

    /// Add uncovered line
    pub fn add_uncovered(&mut self, line: usize) {
        if !self.covered_lines.contains(&line) {
            self.uncovered_lines.insert(line);
        }
        self.executable_lines.insert(line);
    }

    /// Coverage percentage
    pub fn percentage(&self) -> f32 {
        if self.executable_lines.is_empty() {
            return 100.0;
        }
        (self.covered_lines.len() as f32 / self.executable_lines.len() as f32) * 100.0
    }

    /// Is line covered?
    pub fn is_covered(&self, line: usize) -> Option<bool> {
        if self.covered_lines.contains(&line) {
            Some(true)
        } else if self.uncovered_lines.contains(&line) {
            Some(false)
        } else {
            None // Not an executable line
        }
    }

    /// Get gutter symbol for line
    pub fn gutter(&self, line: usize) -> &'static str {
        match self.is_covered(line) {
            Some(true) => "▓",
            Some(false) => "░",
            None => " ",
        }
    }

    /// Get gutter color for line
    pub fn gutter_color(&self, line: usize) -> &'static str {
        match self.is_covered(line) {
            Some(true) => "\x1b[32m",  // Green
            Some(false) => "\x1b[31m", // Red
            None => "",
        }
    }
}

/// Branch coverage info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCoverage {
    /// Line number
    pub line: usize,
    /// Branch index
    pub branch: usize,
    /// Was this branch taken?
    pub taken: bool,
}

/// Coverage tracker for the whole project
#[derive(Debug, Default)]
pub struct CoverageTracker {
    /// Coverage by file
    files: HashMap<PathBuf, FileCoverage>,
    /// Last update
    last_update: Option<DateTime<Utc>>,
}

impl CoverageTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Add file coverage
    pub fn add_file(&mut self, coverage: FileCoverage) {
        self.files.insert(coverage.path.clone(), coverage);
        self.last_update = Some(Utc::now());
    }

    /// Get coverage for a file
    pub fn get(&self, path: &Path) -> Option<&FileCoverage> {
        self.files.get(path)
    }

    /// Overall coverage percentage
    pub fn total_percentage(&self) -> f32 {
        if self.files.is_empty() {
            return 0.0;
        }

        let total_covered: usize = self.files.values().map(|f| f.covered_lines.len()).sum();
        let total_executable: usize = self.files.values().map(|f| f.executable_lines.len()).sum();

        if total_executable == 0 {
            return 100.0;
        }

        (total_covered as f32 / total_executable as f32) * 100.0
    }

    /// Files below threshold
    pub fn below_threshold(&self, threshold: f32) -> Vec<&FileCoverage> {
        self.files
            .values()
            .filter(|f| f.percentage() < threshold)
            .collect()
    }

    /// Files sorted by coverage (ascending)
    pub fn sorted_by_coverage(&self) -> Vec<&FileCoverage> {
        let mut files: Vec<_> = self.files.values().collect();
        files.sort_by(|a, b| {
            a.percentage()
                .partial_cmp(&b.percentage())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        files
    }

    /// Clear all coverage data
    pub fn clear(&mut self) {
        self.files.clear();
        self.last_update = None;
    }

    /// File count
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Progress bar for test runs
#[derive(Debug, Clone)]
pub struct TestProgress {
    /// Total tests
    pub total: usize,
    /// Completed tests
    pub completed: usize,
    /// Passed tests
    pub passed: usize,
    /// Failed tests
    pub failed: usize,
    /// Bar width
    pub width: usize,
    /// Show percentage
    pub show_percentage: bool,
}

impl TestProgress {
    /// Create new progress
    pub fn new(total: usize) -> Self {
        Self {
            total,
            completed: 0,
            passed: 0,
            failed: 0,
            width: 40,
            show_percentage: true,
        }
    }

    /// Update progress
    pub fn update(&mut self, passed: bool) {
        self.completed += 1;
        if passed {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
    }

    /// Completion percentage
    pub fn percentage(&self) -> f32 {
        if self.total == 0 {
            return 100.0;
        }
        (self.completed as f32 / self.total as f32) * 100.0
    }

    /// Render progress bar
    pub fn render(&self) -> String {
        let filled = if self.total > 0 {
            (self.completed * self.width) / self.total
        } else {
            self.width
        };
        let empty = self.width - filled;

        let bar: String = "█".repeat(filled) + &"░".repeat(empty);

        let status = if self.failed > 0 {
            format!("\x1b[31m{} failed\x1b[0m", self.failed)
        } else {
            format!("\x1b[32m{} passed\x1b[0m", self.passed)
        };

        if self.show_percentage {
            format!("[{}] {:.1}% {}", bar, self.percentage(), status)
        } else {
            format!("[{}] {}/{} {}", bar, self.completed, self.total, status)
        }
    }

    /// Is complete?
    pub fn is_complete(&self) -> bool {
        self.completed >= self.total
    }

    /// Is all passing?
    pub fn is_passing(&self) -> bool {
        self.failed == 0
    }
}

/// Sparkline for test history
#[derive(Debug, Clone)]
pub struct TestSparkline {
    /// Data points (success rates)
    points: Vec<f32>,
    /// Max points to keep
    max_points: usize,
}

impl TestSparkline {
    /// Create new sparkline
    pub fn new(max_points: usize) -> Self {
        Self {
            points: Vec::new(),
            max_points,
        }
    }

    /// Add a point
    pub fn add(&mut self, success_rate: f32) {
        self.points.push(success_rate);
        if self.points.len() > self.max_points {
            self.points.remove(0);
        }
    }

    /// Render sparkline
    pub fn render(&self) -> String {
        const BLOCKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

        self.points
            .iter()
            .map(|&p| {
                let idx = ((p / 100.0) * (BLOCKS.len() - 1) as f32) as usize;
                let idx = idx.min(BLOCKS.len() - 1);
                BLOCKS[idx]
            })
            .collect()
    }

    /// Trend direction
    pub fn trend(&self) -> Trend {
        if self.points.len() < 2 {
            return Trend::Stable;
        }

        let recent = self
            .points
            .iter()
            .rev()
            .take(3)
            .copied()
            .collect::<Vec<_>>();
        let avg_recent: f32 = recent.iter().sum::<f32>() / recent.len() as f32;

        let older = self
            .points
            .iter()
            .rev()
            .skip(3)
            .take(3)
            .copied()
            .collect::<Vec<_>>();
        if older.is_empty() {
            return Trend::Stable;
        }
        let avg_older: f32 = older.iter().sum::<f32>() / older.len() as f32;

        if avg_recent > avg_older + 5.0 {
            Trend::Improving
        } else if avg_recent < avg_older - 5.0 {
            Trend::Declining
        } else {
            Trend::Stable
        }
    }

    /// Point count
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Trend direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Improving,
    Stable,
    Declining,
}

impl Trend {
    /// Icon
    pub fn icon(&self) -> &'static str {
        match self {
            Trend::Improving => "↗",
            Trend::Stable => "→",
            Trend::Declining => "↘",
        }
    }

    /// Color
    pub fn color(&self) -> &'static str {
        match self {
            Trend::Improving => "\x1b[32m",
            Trend::Stable => "\x1b[33m",
            Trend::Declining => "\x1b[31m",
        }
    }
}

/// Test dashboard combining all components
#[derive(Debug)]
pub struct TestDashboard {
    /// Test explorer
    pub explorer: TestExplorer,
    /// Watch mode
    pub watch: WatchMode,
    /// Coverage tracker
    pub coverage: CoverageTracker,
    /// Current test run
    pub current_run: Option<TestRun>,
    /// Run history
    pub history: Vec<TestRun>,
    /// Sparkline
    pub sparkline: TestSparkline,
    /// Max history
    max_history: usize,
}

impl Default for TestDashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDashboard {
    /// Create new dashboard
    pub fn new() -> Self {
        Self {
            explorer: TestExplorer::new(),
            watch: WatchMode::new(),
            coverage: CoverageTracker::new(),
            current_run: None,
            history: Vec::new(),
            sparkline: TestSparkline::new(20),
            max_history: 50,
        }
    }

    /// Start a new test run
    pub fn start_run(&mut self) -> &mut TestRun {
        self.current_run = Some(TestRun::new());
        self.current_run.as_mut().unwrap()
    }

    /// Complete current run
    pub fn complete_run(&mut self) {
        if let Some(mut run) = self.current_run.take() {
            run.complete();

            // Update sparkline
            let summary = run.summary();
            self.sparkline.add(summary.success_rate());

            // Add to history
            self.history.push(run);
            if self.history.len() > self.max_history {
                self.history.remove(0);
            }
        }
    }

    /// Get last run
    pub fn last_run(&self) -> Option<&TestRun> {
        self.history.last()
    }

    /// Get failure streak (consecutive failing runs)
    pub fn failure_streak(&self) -> usize {
        self.history
            .iter()
            .rev()
            .take_while(|r| r.status == RunStatus::Failed)
            .count()
    }

    /// Get success streak
    pub fn success_streak(&self) -> usize {
        self.history
            .iter()
            .rev()
            .take_while(|r| r.status == RunStatus::Passed)
            .count()
    }

    /// Update explorer with test results
    pub fn update_explorer(&mut self, tests: &[Test]) {
        self.explorer.build_tree(tests);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_status_icons() {
        assert_eq!(TestStatus::Pending.icon(), "○");
        assert_eq!(TestStatus::Running.icon(), "◐");
        assert_eq!(TestStatus::Passed.icon(), "✓");
        assert_eq!(TestStatus::Failed.icon(), "✗");
        assert_eq!(TestStatus::Ignored.icon(), "⊘");
        assert_eq!(TestStatus::Skipped.icon(), "⊝");
    }

    #[test]
    fn test_test_status_colors() {
        assert!(TestStatus::Passed.color().contains("32"));
        assert!(TestStatus::Failed.color().contains("31"));
    }

    #[test]
    fn test_test_status_is_final() {
        assert!(!TestStatus::Pending.is_final());
        assert!(!TestStatus::Running.is_final());
        assert!(TestStatus::Passed.is_final());
        assert!(TestStatus::Failed.is_final());
        assert!(TestStatus::Ignored.is_final());
    }

    #[test]
    fn test_test_status_default() {
        let s: TestStatus = Default::default();
        assert_eq!(s, TestStatus::Pending);
    }

    #[test]
    fn test_test_status_display() {
        assert_eq!(format!("{}", TestStatus::Passed), "Passed");
        assert_eq!(format!("{}", TestStatus::Failed), "Failed");
    }

    #[test]
    fn test_test_creation() {
        let t = Test::new("module::submodule::test_name".to_string());
        assert_eq!(t.function, "test_name");
        assert_eq!(t.module, "module::submodule");
        assert_eq!(t.status, TestStatus::Pending);
    }

    #[test]
    fn test_test_with_location() {
        let t = Test::new("test".to_string()).with_location(PathBuf::from("test.rs"), 42);
        assert_eq!(t.file, Some(PathBuf::from("test.rs")));
        assert_eq!(t.line, Some(42));
    }

    #[test]
    fn test_test_lifecycle() {
        let mut t = Test::new("test".to_string());
        t.start();
        assert_eq!(t.status, TestStatus::Running);

        t.pass(Duration::milliseconds(100));
        assert_eq!(t.status, TestStatus::Passed);
        assert!(t.duration.is_some());
    }

    #[test]
    fn test_test_failure() {
        let mut t = Test::new("test".to_string());
        t.fail(Duration::milliseconds(50), "assertion failed".to_string());
        assert_eq!(t.status, TestStatus::Failed);
        assert!(t.failure_message.is_some());
    }

    #[test]
    fn test_test_ignore() {
        let mut t = Test::new("test".to_string());
        t.ignore();
        assert_eq!(t.status, TestStatus::Ignored);
        assert!(t.ignored);
    }

    #[test]
    fn test_test_display() {
        let mut t = Test::new("module::test_fn".to_string());
        t.pass(Duration::seconds(1));
        let display = t.display();
        assert!(display.contains("✓"));
        assert!(display.contains("module::test_fn"));
    }

    #[test]
    fn test_test_short_display() {
        let t = Test::new("module::test_fn".to_string());
        let display = t.short_display();
        assert!(display.contains("test_fn"));
    }

    #[test]
    fn test_test_suite_creation() {
        let suite = TestSuite::new("test_suite".to_string());
        assert_eq!(suite.name, "test_suite");
        assert_eq!(suite.total, 0);
    }

    #[test]
    fn test_test_suite_add_test() {
        let mut suite = TestSuite::new("suite".to_string());
        let mut t = Test::new("test".to_string());
        t.pass(Duration::milliseconds(10));
        suite.add_test(t);

        assert_eq!(suite.total, 1);
        assert_eq!(suite.passed, 1);
    }

    #[test]
    fn test_test_suite_add_child() {
        let mut parent = TestSuite::new("parent".to_string());
        let mut child = TestSuite::new("child".to_string());
        child.add_test(Test::new("test".to_string()));
        parent.add_child(child);

        assert_eq!(parent.total, 1);
    }

    #[test]
    fn test_test_suite_all_tests() {
        let mut suite = TestSuite::new("suite".to_string());
        suite.add_test(Test::new("t1".to_string()));
        suite.add_test(Test::new("t2".to_string()));

        let all = suite.all_tests();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_test_suite_failed_tests() {
        let mut suite = TestSuite::new("suite".to_string());
        let mut t1 = Test::new("t1".to_string());
        t1.pass(Duration::milliseconds(10));
        suite.add_test(t1);

        let mut t2 = Test::new("t2".to_string());
        t2.fail(Duration::milliseconds(10), "failed".to_string());
        suite.add_test(t2);

        assert_eq!(suite.failed_tests().len(), 1);
        assert_eq!(suite.passed_tests().len(), 1);
    }

    #[test]
    fn test_test_suite_is_passing() {
        let mut suite = TestSuite::new("suite".to_string());
        let mut t = Test::new("t".to_string());
        t.pass(Duration::milliseconds(10));
        suite.add_test(t);

        assert!(suite.is_passing());
    }

    #[test]
    fn test_test_suite_progress() {
        let mut suite = TestSuite::new("suite".to_string());
        let mut t = Test::new("t".to_string());
        t.pass(Duration::milliseconds(10));
        suite.add_test(t);
        suite.add_test(Test::new("t2".to_string())); // Pending

        assert_eq!(suite.progress(), 50.0);
    }

    #[test]
    fn test_test_suite_summary() {
        let suite = TestSuite::new("suite".to_string());
        let summary = suite.summary();
        assert!(summary.contains("suite"));
        assert!(summary.contains("total"));
    }

    #[test]
    fn test_test_run_creation() {
        let run = TestRun::new();
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.ended_at.is_none());
    }

    #[test]
    fn test_test_run_add_result() {
        let mut run = TestRun::new();
        run.add_result(Test::new("test".to_string()));
        assert_eq!(run.results.len(), 1);
    }

    #[test]
    fn test_test_run_complete() {
        let mut run = TestRun::new();
        let mut t = Test::new("test".to_string());
        t.pass(Duration::milliseconds(10));
        run.add_result(t);
        run.complete();

        assert_eq!(run.status, RunStatus::Passed);
        assert!(run.ended_at.is_some());
    }

    #[test]
    fn test_test_run_complete_with_failures() {
        let mut run = TestRun::new();
        let mut t = Test::new("test".to_string());
        t.fail(Duration::milliseconds(10), "failed".to_string());
        run.add_result(t);
        run.complete();

        assert_eq!(run.status, RunStatus::Failed);
    }

    #[test]
    fn test_test_run_cancel() {
        let mut run = TestRun::new();
        run.cancel();
        assert_eq!(run.status, RunStatus::Cancelled);
    }

    #[test]
    fn test_test_run_summary() {
        let mut run = TestRun::new();
        let mut t = Test::new("test".to_string());
        t.pass(Duration::milliseconds(10));
        run.add_result(t);
        run.complete();

        let summary = run.summary();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 1);
    }

    #[test]
    fn test_test_run_summary_success_rate() {
        let summary = TestRunSummary {
            total: 10,
            passed: 8,
            failed: 2,
            ignored: 0,
            duration: None,
        };
        assert_eq!(summary.success_rate(), 80.0);
    }

    #[test]
    fn test_test_run_summary_display() {
        let summary = TestRunSummary {
            total: 10,
            passed: 8,
            failed: 2,
            ignored: 0,
            duration: Some(Duration::seconds(1)),
        };
        let display = summary.display();
        assert!(display.contains("8 passed"));
        assert!(display.contains("2 failed"));
    }

    #[test]
    fn test_test_tree_node_module() {
        let node = TestTreeNode::module("module".to_string(), "module".to_string());
        assert!(!node.is_test);
        assert!(node.status.is_none());
    }

    #[test]
    fn test_test_tree_node_test() {
        let node = TestTreeNode::test(
            "test".to_string(),
            "module::test".to_string(),
            TestStatus::Passed,
        );
        assert!(node.is_test);
        assert_eq!(node.status, Some(TestStatus::Passed));
    }

    #[test]
    fn test_test_tree_node_add_child() {
        let mut node = TestTreeNode::module("mod".to_string(), "mod".to_string());
        node.add_child(TestTreeNode::test(
            "test".to_string(),
            "mod::test".to_string(),
            TestStatus::Pending,
        ));
        assert_eq!(node.children.len(), 1);
    }

    #[test]
    fn test_test_tree_node_find_or_create() {
        let mut root = TestTreeNode::module(String::new(), String::new());
        let _ = root.find_or_create(&["a", "b", "c"]);
        assert!(!root.children.is_empty());
    }

    #[test]
    fn test_test_tree_node_toggle() {
        let mut node = TestTreeNode::module("mod".to_string(), "mod".to_string());
        assert!(node.expanded);
        node.toggle();
        assert!(!node.expanded);
    }

    #[test]
    fn test_test_tree_node_test_count() {
        let mut node = TestTreeNode::module("mod".to_string(), "mod".to_string());
        node.add_child(TestTreeNode::test(
            "t1".to_string(),
            "mod::t1".to_string(),
            TestStatus::Passed,
        ));
        node.add_child(TestTreeNode::test(
            "t2".to_string(),
            "mod::t2".to_string(),
            TestStatus::Failed,
        ));
        assert_eq!(node.test_count(), 2);
    }

    #[test]
    fn test_test_tree_node_failed_count() {
        let mut node = TestTreeNode::module("mod".to_string(), "mod".to_string());
        node.add_child(TestTreeNode::test(
            "t1".to_string(),
            "mod::t1".to_string(),
            TestStatus::Passed,
        ));
        node.add_child(TestTreeNode::test(
            "t2".to_string(),
            "mod::t2".to_string(),
            TestStatus::Failed,
        ));
        assert_eq!(node.failed_count(), 1);
    }

    #[test]
    fn test_test_tree_node_aggregate_status() {
        let mut node = TestTreeNode::module("mod".to_string(), "mod".to_string());
        node.add_child(TestTreeNode::test(
            "t1".to_string(),
            "mod::t1".to_string(),
            TestStatus::Passed,
        ));
        assert_eq!(node.aggregate_status(), TestStatus::Passed);
    }

    #[test]
    fn test_test_tree_node_aggregate_failed() {
        let mut node = TestTreeNode::module("mod".to_string(), "mod".to_string());
        node.add_child(TestTreeNode::test(
            "t1".to_string(),
            "mod::t1".to_string(),
            TestStatus::Failed,
        ));
        assert_eq!(node.aggregate_status(), TestStatus::Failed);
    }

    #[test]
    fn test_test_explorer_new() {
        let explorer = TestExplorer::new();
        assert!(explorer.selected().is_none());
    }

    #[test]
    fn test_test_explorer_build_tree() {
        let mut explorer = TestExplorer::new();
        explorer.build_tree(&[
            Test::new("mod::test1".to_string()),
            Test::new("mod::test2".to_string()),
        ]);
        let visible = explorer.visible_nodes();
        assert!(!visible.is_empty());
    }

    #[test]
    fn test_test_explorer_filter() {
        let mut explorer = TestExplorer::new();
        explorer.build_tree(&[
            Test::new("mod::test1".to_string()),
            Test::new("other::test2".to_string()),
        ]);
        explorer.set_filter(Some("mod".to_string()));
        // Filter is applied
    }

    #[test]
    fn test_test_explorer_toggle_failed_only() {
        let mut explorer = TestExplorer::new();
        assert!(!explorer.show_failed_only);
        explorer.toggle_failed_only();
        assert!(explorer.show_failed_only);
    }

    #[test]
    fn test_test_explorer_select() {
        let mut explorer = TestExplorer::new();
        explorer.select("mod::test".to_string());
        assert_eq!(explorer.selected(), Some("mod::test"));
    }

    #[test]
    fn test_watch_mode_new() {
        let watch = WatchMode::new();
        assert!(!watch.is_active());
    }

    #[test]
    fn test_watch_mode_start_stop() {
        let mut watch = WatchMode::new();
        watch.start();
        assert!(watch.is_active());
        watch.stop();
        assert!(!watch.is_active());
    }

    #[test]
    fn test_watch_mode_debounce() {
        let watch = WatchMode::new().with_debounce(1000);
        assert_eq!(watch.debounce, Duration::milliseconds(1000));
    }

    #[test]
    fn test_watch_mode_file_changed() {
        let mut watch = WatchMode::new();
        watch.start();
        watch.file_changed(PathBuf::from("test.rs"));
        assert!(!watch.pending_changes.is_empty());
    }

    #[test]
    fn test_watch_mode_should_run() {
        let mut watch = WatchMode::new();
        watch.start();
        watch.file_changed(PathBuf::from("test.rs"));
        assert!(watch.should_run());
    }

    #[test]
    fn test_watch_mode_take_changes() {
        let mut watch = WatchMode::new();
        watch.start();
        watch.file_changed(PathBuf::from("test.rs"));
        let changes = watch.take_changes();
        assert!(!changes.is_empty());
        assert!(watch.pending_changes.is_empty());
    }

    #[test]
    fn test_watch_mode_patterns() {
        let mut watch = WatchMode::new();
        watch.add_pattern("test_".to_string());
        assert_eq!(watch.patterns().len(), 1);
        watch.clear_patterns();
        assert!(watch.patterns().is_empty());
    }

    #[test]
    fn test_watch_mode_toggle_run_on_save() {
        let mut watch = WatchMode::new();
        assert!(watch.run_on_save);
        watch.toggle_run_on_save();
        assert!(!watch.run_on_save);
    }

    #[test]
    fn test_file_coverage_new() {
        let cov = FileCoverage::new(PathBuf::from("test.rs"));
        assert_eq!(cov.path, PathBuf::from("test.rs"));
        assert!(cov.covered_lines.is_empty());
    }

    #[test]
    fn test_file_coverage_cover_line() {
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(10);
        assert!(cov.covered_lines.contains(&10));
        assert!(cov.executable_lines.contains(&10));
    }

    #[test]
    fn test_file_coverage_add_uncovered() {
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.add_uncovered(5);
        assert!(cov.uncovered_lines.contains(&5));
    }

    #[test]
    fn test_file_coverage_percentage() {
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(1);
        cov.cover_line(2);
        cov.add_uncovered(3);
        cov.add_uncovered(4);

        assert_eq!(cov.percentage(), 50.0);
    }

    #[test]
    fn test_file_coverage_is_covered() {
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(1);
        cov.add_uncovered(2);

        assert_eq!(cov.is_covered(1), Some(true));
        assert_eq!(cov.is_covered(2), Some(false));
        assert_eq!(cov.is_covered(3), None);
    }

    #[test]
    fn test_file_coverage_gutter() {
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(1);
        cov.add_uncovered(2);

        assert_eq!(cov.gutter(1), "▓");
        assert_eq!(cov.gutter(2), "░");
        assert_eq!(cov.gutter(3), " ");
    }

    #[test]
    fn test_coverage_tracker_new() {
        let tracker = CoverageTracker::new();
        assert_eq!(tracker.file_count(), 0);
    }

    #[test]
    fn test_coverage_tracker_add_file() {
        let mut tracker = CoverageTracker::new();
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(1);
        tracker.add_file(cov);

        assert_eq!(tracker.file_count(), 1);
        assert!(tracker.get(Path::new("test.rs")).is_some());
    }

    #[test]
    fn test_coverage_tracker_total_percentage() {
        let mut tracker = CoverageTracker::new();
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(1);
        cov.add_uncovered(2);
        tracker.add_file(cov);

        assert_eq!(tracker.total_percentage(), 50.0);
    }

    #[test]
    fn test_coverage_tracker_below_threshold() {
        let mut tracker = CoverageTracker::new();
        let mut cov = FileCoverage::new(PathBuf::from("test.rs"));
        cov.cover_line(1);
        cov.add_uncovered(2);
        cov.add_uncovered(3);
        cov.add_uncovered(4);
        tracker.add_file(cov);

        let below = tracker.below_threshold(50.0);
        assert_eq!(below.len(), 1);
    }

    #[test]
    fn test_coverage_tracker_sorted() {
        let mut tracker = CoverageTracker::new();

        let mut cov1 = FileCoverage::new(PathBuf::from("a.rs"));
        cov1.cover_line(1);
        cov1.add_uncovered(2);
        tracker.add_file(cov1);

        let mut cov2 = FileCoverage::new(PathBuf::from("b.rs"));
        cov2.cover_line(1);
        tracker.add_file(cov2);

        let sorted = tracker.sorted_by_coverage();
        assert_eq!(sorted.len(), 2);
        assert!(sorted[0].percentage() <= sorted[1].percentage());
    }

    #[test]
    fn test_coverage_tracker_clear() {
        let mut tracker = CoverageTracker::new();
        tracker.add_file(FileCoverage::new(PathBuf::from("test.rs")));
        tracker.clear();
        assert_eq!(tracker.file_count(), 0);
    }

    #[test]
    fn test_test_progress_new() {
        let progress = TestProgress::new(10);
        assert_eq!(progress.total, 10);
        assert_eq!(progress.completed, 0);
    }

    #[test]
    fn test_test_progress_update() {
        let mut progress = TestProgress::new(10);
        progress.update(true);
        progress.update(false);

        assert_eq!(progress.completed, 2);
        assert_eq!(progress.passed, 1);
        assert_eq!(progress.failed, 1);
    }

    #[test]
    fn test_test_progress_percentage() {
        let mut progress = TestProgress::new(10);
        progress.update(true);
        progress.update(true);

        assert_eq!(progress.percentage(), 20.0);
    }

    #[test]
    fn test_test_progress_render() {
        let mut progress = TestProgress::new(10);
        progress.update(true);
        let render = progress.render();
        assert!(render.contains("█"));
        assert!(render.contains("passed"));
    }

    #[test]
    fn test_test_progress_is_complete() {
        let mut progress = TestProgress::new(2);
        assert!(!progress.is_complete());
        progress.update(true);
        progress.update(true);
        assert!(progress.is_complete());
    }

    #[test]
    fn test_test_progress_is_passing() {
        let mut progress = TestProgress::new(2);
        progress.update(true);
        assert!(progress.is_passing());
        progress.update(false);
        assert!(!progress.is_passing());
    }

    #[test]
    fn test_test_sparkline_new() {
        let sparkline = TestSparkline::new(10);
        assert!(sparkline.is_empty());
    }

    #[test]
    fn test_test_sparkline_add() {
        let mut sparkline = TestSparkline::new(5);
        sparkline.add(100.0);
        sparkline.add(50.0);
        assert_eq!(sparkline.len(), 2);
    }

    #[test]
    fn test_test_sparkline_max_points() {
        let mut sparkline = TestSparkline::new(3);
        sparkline.add(100.0);
        sparkline.add(90.0);
        sparkline.add(80.0);
        sparkline.add(70.0);
        assert_eq!(sparkline.len(), 3);
    }

    #[test]
    fn test_test_sparkline_render() {
        let mut sparkline = TestSparkline::new(5);
        sparkline.add(100.0);
        sparkline.add(50.0);
        sparkline.add(0.0);
        let render = sparkline.render();
        assert!(!render.is_empty());
    }

    #[test]
    fn test_test_sparkline_trend() {
        let mut sparkline = TestSparkline::new(10);
        // Stable
        assert_eq!(sparkline.trend(), Trend::Stable);

        // Improving
        for i in 0..6 {
            sparkline.add(50.0 + i as f32 * 10.0);
        }
        assert_eq!(sparkline.trend(), Trend::Improving);
    }

    #[test]
    fn test_test_sparkline_declining_trend() {
        let mut sparkline = TestSparkline::new(10);
        for i in 0..6 {
            sparkline.add(100.0 - i as f32 * 15.0);
        }
        assert_eq!(sparkline.trend(), Trend::Declining);
    }

    #[test]
    fn test_trend_icon() {
        assert_eq!(Trend::Improving.icon(), "↗");
        assert_eq!(Trend::Stable.icon(), "→");
        assert_eq!(Trend::Declining.icon(), "↘");
    }

    #[test]
    fn test_trend_color() {
        assert!(Trend::Improving.color().contains("32"));
        assert!(Trend::Declining.color().contains("31"));
    }

    #[test]
    fn test_test_dashboard_new() {
        let dashboard = TestDashboard::new();
        assert!(dashboard.current_run.is_none());
        assert!(dashboard.history.is_empty());
    }

    #[test]
    fn test_test_dashboard_start_run() {
        let mut dashboard = TestDashboard::new();
        let run = dashboard.start_run();
        assert_eq!(run.status, RunStatus::Running);
    }

    #[test]
    fn test_test_dashboard_complete_run() {
        let mut dashboard = TestDashboard::new();
        {
            let run = dashboard.start_run();
            let mut t = Test::new("test".to_string());
            t.pass(Duration::milliseconds(10));
            run.add_result(t);
        }
        dashboard.complete_run();

        assert!(dashboard.current_run.is_none());
        assert_eq!(dashboard.history.len(), 1);
    }

    #[test]
    fn test_test_dashboard_last_run() {
        let mut dashboard = TestDashboard::new();
        dashboard.start_run();
        dashboard.complete_run();

        assert!(dashboard.last_run().is_some());
    }

    #[test]
    fn test_test_dashboard_streaks() {
        let mut dashboard = TestDashboard::new();

        // Add passing runs
        for _ in 0..3 {
            let run = dashboard.start_run();
            let mut t = Test::new("test".to_string());
            t.pass(Duration::milliseconds(10));
            run.add_result(t);
            dashboard.complete_run();
        }

        assert_eq!(dashboard.success_streak(), 3);
        assert_eq!(dashboard.failure_streak(), 0);
    }

    #[test]
    fn test_test_dashboard_update_explorer() {
        let mut dashboard = TestDashboard::new();
        dashboard.update_explorer(&[
            Test::new("mod::test1".to_string()),
            Test::new("mod::test2".to_string()),
        ]);
        // Explorer should have nodes
        let visible = dashboard.explorer.visible_nodes();
        assert!(!visible.is_empty());
    }
}
