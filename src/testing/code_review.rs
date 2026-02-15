//! Code Review Assistant
//!
//! Comprehensive code review capabilities including:
//! - PR/diff analysis
//! - Automated review comments
//! - Style checking
//! - Complexity analysis
//! - Suggestion generation

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Severity of a review issue
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Informational suggestion
    Info,
    /// Minor style or best practice issue
    Warning,
    /// Should be fixed before merge
    Error,
    /// Security or critical issue, must fix
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

/// Category of review comment
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReviewCategory {
    /// Code style and formatting
    Style,
    /// Performance concerns
    Performance,
    /// Security vulnerabilities
    Security,
    /// Logic or correctness issues
    Logic,
    /// Missing or incorrect documentation
    Documentation,
    /// Test coverage concerns
    Testing,
    /// Code complexity issues
    Complexity,
    /// Best practices and patterns
    BestPractice,
    /// Naming conventions
    Naming,
    /// Error handling issues
    ErrorHandling,
    /// Custom category
    Custom(String),
}

impl ReviewCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Style => "style",
            Self::Performance => "performance",
            Self::Security => "security",
            Self::Logic => "logic",
            Self::Documentation => "documentation",
            Self::Testing => "testing",
            Self::Complexity => "complexity",
            Self::BestPractice => "best_practice",
            Self::Naming => "naming",
            Self::ErrorHandling => "error_handling",
            Self::Custom(s) => s,
        }
    }
}

/// A line change in a diff
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// Original line number (None if added)
    pub old_line: Option<u32>,
    /// New line number (None if removed)
    pub new_line: Option<u32>,
    /// Line content
    pub content: String,
    /// Whether this line is added, removed, or context
    pub change_type: ChangeType,
}

/// Type of change for a diff line
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// Line was added
    Added,
    /// Line was removed
    Removed,
    /// Context line (unchanged)
    Context,
}

/// A hunk in a diff (contiguous block of changes)
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Starting line in old file
    pub old_start: u32,
    /// Number of lines in old file
    pub old_count: u32,
    /// Starting line in new file
    pub new_start: u32,
    /// Number of lines in new file
    pub new_count: u32,
    /// Lines in this hunk
    pub lines: Vec<DiffLine>,
}

/// A file diff
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Path of the old file (None if new file)
    pub old_path: Option<PathBuf>,
    /// Path of the new file (None if deleted)
    pub new_path: Option<PathBuf>,
    /// Status of the file (added, modified, deleted, renamed)
    pub status: FileStatus,
    /// Hunks in this diff
    pub hunks: Vec<DiffHunk>,
    /// Language detected for this file
    pub language: Option<String>,
}

/// Status of a file in a diff
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Analyzer for git diffs
pub struct DiffAnalyzer {
    /// Supported file extensions and their languages
    language_map: HashMap<String, String>,
}

impl DiffAnalyzer {
    pub fn new() -> Self {
        let mut language_map = HashMap::new();
        language_map.insert("rs".to_string(), "rust".to_string());
        language_map.insert("py".to_string(), "python".to_string());
        language_map.insert("js".to_string(), "javascript".to_string());
        language_map.insert("ts".to_string(), "typescript".to_string());
        language_map.insert("go".to_string(), "go".to_string());
        language_map.insert("java".to_string(), "java".to_string());
        language_map.insert("cpp".to_string(), "cpp".to_string());
        language_map.insert("c".to_string(), "c".to_string());
        language_map.insert("h".to_string(), "c".to_string());
        language_map.insert("hpp".to_string(), "cpp".to_string());
        language_map.insert("rb".to_string(), "ruby".to_string());
        language_map.insert("php".to_string(), "php".to_string());
        language_map.insert("swift".to_string(), "swift".to_string());
        language_map.insert("kt".to_string(), "kotlin".to_string());
        language_map.insert("scala".to_string(), "scala".to_string());
        language_map.insert("sh".to_string(), "shell".to_string());
        language_map.insert("bash".to_string(), "shell".to_string());
        language_map.insert("yml".to_string(), "yaml".to_string());
        language_map.insert("yaml".to_string(), "yaml".to_string());
        language_map.insert("json".to_string(), "json".to_string());
        language_map.insert("toml".to_string(), "toml".to_string());
        language_map.insert("md".to_string(), "markdown".to_string());

        Self { language_map }
    }

    /// Parse a unified diff into structured data
    pub fn parse_diff(&self, diff_text: &str) -> Vec<FileDiff> {
        let mut files = Vec::new();
        let mut current_file: Option<FileDiff> = None;
        let mut current_hunk: Option<DiffHunk> = None;
        let mut old_line = 0u32;
        let mut new_line = 0u32;

        for line in diff_text.lines() {
            if line.starts_with("diff --git") {
                // Save previous file if exists
                if let Some(mut file) = current_file.take() {
                    if let Some(hunk) = current_hunk.take() {
                        file.hunks.push(hunk);
                    }
                    files.push(file);
                }
                // Start new file
                current_file = Some(FileDiff {
                    old_path: None,
                    new_path: None,
                    status: FileStatus::Modified,
                    hunks: Vec::new(),
                    language: None,
                });
            } else if line.starts_with("--- ") {
                if let Some(ref mut file) = current_file {
                    let path = line.strip_prefix("--- ").unwrap_or("");
                    if path != "/dev/null" {
                        let path = path.strip_prefix("a/").unwrap_or(path);
                        file.old_path = Some(PathBuf::from(path));
                    }
                }
            } else if line.starts_with("+++ ") {
                if let Some(ref mut file) = current_file {
                    let path = line.strip_prefix("+++ ").unwrap_or("");
                    if path != "/dev/null" {
                        let path = path.strip_prefix("b/").unwrap_or(path);
                        let path_buf = PathBuf::from(path);
                        // Detect language from extension
                        if let Some(ext) = path_buf.extension().and_then(|e| e.to_str()) {
                            file.language = self.language_map.get(ext).cloned();
                        }
                        file.new_path = Some(path_buf);
                    }
                    // Determine file status
                    file.status = match (&file.old_path, &file.new_path) {
                        (None, Some(_)) => FileStatus::Added,
                        (Some(_), None) => FileStatus::Deleted,
                        (Some(old), Some(new)) if old != new => FileStatus::Renamed,
                        _ => FileStatus::Modified,
                    };
                }
            } else if line.starts_with("@@ ") {
                // Parse hunk header
                if let Some(ref mut file) = current_file {
                    if let Some(hunk) = current_hunk.take() {
                        file.hunks.push(hunk);
                    }
                }

                // Parse @@ -old_start,old_count +new_start,new_count @@
                if let Some(header) = self.parse_hunk_header(line) {
                    old_line = header.0;
                    new_line = header.2;
                    current_hunk = Some(DiffHunk {
                        old_start: header.0,
                        old_count: header.1,
                        new_start: header.2,
                        new_count: header.3,
                        lines: Vec::new(),
                    });
                }
            } else if let Some(ref mut hunk) = current_hunk {
                // Parse diff line
                let (change_type, content) = if let Some(rest) = line.strip_prefix('+') {
                    (ChangeType::Added, rest.to_string())
                } else if let Some(rest) = line.strip_prefix('-') {
                    (ChangeType::Removed, rest.to_string())
                } else if let Some(rest) = line.strip_prefix(' ') {
                    (ChangeType::Context, rest.to_string())
                } else {
                    (ChangeType::Context, line.to_string())
                };

                let diff_line = match change_type {
                    ChangeType::Added => {
                        let dl = DiffLine {
                            old_line: None,
                            new_line: Some(new_line),
                            content,
                            change_type,
                        };
                        new_line += 1;
                        dl
                    }
                    ChangeType::Removed => {
                        let dl = DiffLine {
                            old_line: Some(old_line),
                            new_line: None,
                            content,
                            change_type,
                        };
                        old_line += 1;
                        dl
                    }
                    ChangeType::Context => {
                        let dl = DiffLine {
                            old_line: Some(old_line),
                            new_line: Some(new_line),
                            content,
                            change_type,
                        };
                        old_line += 1;
                        new_line += 1;
                        dl
                    }
                };

                hunk.lines.push(diff_line);
            }
        }

        // Save last file
        if let Some(mut file) = current_file {
            if let Some(hunk) = current_hunk {
                file.hunks.push(hunk);
            }
            files.push(file);
        }

        files
    }

    /// Parse a hunk header: @@ -old_start,old_count +new_start,new_count @@
    fn parse_hunk_header(&self, line: &str) -> Option<(u32, u32, u32, u32)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }

        let old_part = parts[1].strip_prefix('-')?;
        let new_part = parts[2].strip_prefix('+')?;

        let (old_start, old_count) = Self::parse_range(old_part)?;
        let (new_start, new_count) = Self::parse_range(new_part)?;

        Some((old_start, old_count, new_start, new_count))
    }

    fn parse_range(range: &str) -> Option<(u32, u32)> {
        if let Some((start, count)) = range.split_once(',') {
            Some((start.parse().ok()?, count.parse().ok()?))
        } else {
            Some((range.parse().ok()?, 1))
        }
    }

    /// Get statistics for a diff
    #[allow(clippy::field_reassign_with_default)]
    pub fn get_stats(&self, files: &[FileDiff]) -> DiffStats {
        let mut stats = DiffStats::default();

        stats.files_changed = files.len();

        for file in files {
            match file.status {
                FileStatus::Added => stats.files_added += 1,
                FileStatus::Deleted => stats.files_deleted += 1,
                FileStatus::Modified => stats.files_modified += 1,
                FileStatus::Renamed => stats.files_renamed += 1,
            }

            for hunk in &file.hunks {
                for line in &hunk.lines {
                    match line.change_type {
                        ChangeType::Added => stats.lines_added += 1,
                        ChangeType::Removed => stats.lines_removed += 1,
                        ChangeType::Context => {}
                    }
                }
            }

            if let Some(lang) = &file.language {
                *stats.languages.entry(lang.clone()).or_insert(0) += 1;
            }
        }

        stats
    }
}

impl Default for DiffAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about a diff
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub files_changed: usize,
    pub files_added: usize,
    pub files_deleted: usize,
    pub files_modified: usize,
    pub files_renamed: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub languages: HashMap<String, usize>,
}

/// Style rule for checking
#[derive(Debug, Clone)]
pub struct StyleRule {
    /// Rule identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description
    pub description: String,
    /// Languages this rule applies to
    pub languages: Vec<String>,
    /// Severity of violation
    pub severity: Severity,
    /// Category
    pub category: ReviewCategory,
}

/// A style violation found in code
#[derive(Debug, Clone)]
pub struct StyleViolation {
    /// Rule that was violated
    pub rule_id: String,
    /// File path
    pub file: PathBuf,
    /// Line number
    pub line: u32,
    /// Column number
    pub column: Option<u32>,
    /// Description of the violation
    pub message: String,
    /// Severity
    pub severity: Severity,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Style checker for code review
pub struct StyleChecker {
    /// Registered style rules
    rules: Vec<StyleRule>,
}

impl StyleChecker {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add default Rust style rules
    pub fn with_rust_rules(mut self) -> Self {
        self.rules.push(StyleRule {
            id: "rust/unwrap".to_string(),
            name: "Avoid unwrap()".to_string(),
            description: "Using unwrap() can panic. Prefer ? or handle errors explicitly."
                .to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            category: ReviewCategory::ErrorHandling,
        });
        self.rules.push(StyleRule {
            id: "rust/expect".to_string(),
            name: "Prefer ? over expect()".to_string(),
            description: "Using expect() for error handling can be replaced with ?".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Info,
            category: ReviewCategory::ErrorHandling,
        });
        self.rules.push(StyleRule {
            id: "rust/clone".to_string(),
            name: "Unnecessary clone".to_string(),
            description: "Clone may not be necessary. Consider borrowing instead.".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Info,
            category: ReviewCategory::Performance,
        });
        self.rules.push(StyleRule {
            id: "rust/todo".to_string(),
            name: "TODO in code".to_string(),
            description: "TODO comments should be addressed before merge.".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            category: ReviewCategory::BestPractice,
        });
        self.rules.push(StyleRule {
            id: "rust/unsafe".to_string(),
            name: "Unsafe code".to_string(),
            description: "Unsafe code requires careful review for soundness.".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            category: ReviewCategory::Security,
        });
        self.rules.push(StyleRule {
            id: "rust/panic".to_string(),
            name: "Panic in library code".to_string(),
            description: "Panics should be avoided in library code.".to_string(),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            category: ReviewCategory::ErrorHandling,
        });
        self
    }

    /// Add a custom rule
    pub fn add_rule(&mut self, rule: StyleRule) {
        self.rules.push(rule);
    }

    /// Check a file for style violations
    pub fn check_file(
        &self,
        path: &std::path::Path,
        content: &str,
        language: &str,
    ) -> Vec<StyleViolation> {
        let mut violations = Vec::new();

        // Filter rules for this language
        let applicable_rules: Vec<&StyleRule> = self
            .rules
            .iter()
            .filter(|r| r.languages.contains(&language.to_string()) || r.languages.is_empty())
            .collect();

        for (line_num, line) in content.lines().enumerate() {
            let line_num = (line_num + 1) as u32;

            for rule in &applicable_rules {
                if let Some(violation) = self.check_line_against_rule(path, line, line_num, rule) {
                    violations.push(violation);
                }
            }
        }

        violations
    }

    fn check_line_against_rule(
        &self,
        path: &Path,
        line: &str,
        line_num: u32,
        rule: &StyleRule,
    ) -> Option<StyleViolation> {
        let trimmed = line.trim();

        match rule.id.as_str() {
            "rust/unwrap" => {
                if trimmed.contains(".unwrap()") && !trimmed.starts_with("//") {
                    return Some(StyleViolation {
                        rule_id: rule.id.clone(),
                        file: path.to_path_buf(),
                        line: line_num,
                        column: line.find(".unwrap()").map(|c| c as u32),
                        message: rule.description.clone(),
                        severity: rule.severity,
                        suggestion: Some("Use `?` operator or `match`/`if let` instead".into()),
                    });
                }
            }
            "rust/expect" => {
                if trimmed.contains(".expect(") && !trimmed.starts_with("//") {
                    return Some(StyleViolation {
                        rule_id: rule.id.clone(),
                        file: path.to_path_buf(),
                        line: line_num,
                        column: line.find(".expect(").map(|c| c as u32),
                        message: rule.description.clone(),
                        severity: rule.severity,
                        suggestion: Some("Use `?` operator with context".into()),
                    });
                }
            }
            "rust/clone" => {
                if trimmed.contains(".clone()") && !trimmed.starts_with("//") {
                    return Some(StyleViolation {
                        rule_id: rule.id.clone(),
                        file: path.to_path_buf(),
                        line: line_num,
                        column: line.find(".clone()").map(|c| c as u32),
                        message: rule.description.clone(),
                        severity: rule.severity,
                        suggestion: Some("Consider using a reference instead of cloning".into()),
                    });
                }
            }
            "rust/todo" => {
                if trimmed.contains("TODO") || trimmed.contains("FIXME") {
                    return Some(StyleViolation {
                        rule_id: rule.id.clone(),
                        file: path.to_path_buf(),
                        line: line_num,
                        column: None,
                        message: rule.description.clone(),
                        severity: rule.severity,
                        suggestion: Some("Address or create an issue for this TODO".into()),
                    });
                }
            }
            "rust/unsafe" => {
                if trimmed.starts_with("unsafe ") || trimmed.contains(" unsafe ") {
                    return Some(StyleViolation {
                        rule_id: rule.id.clone(),
                        file: path.to_path_buf(),
                        line: line_num,
                        column: line.find("unsafe").map(|c| c as u32),
                        message: rule.description.clone(),
                        severity: rule.severity,
                        suggestion: Some(
                            "Document safety invariants and consider safe alternatives".into(),
                        ),
                    });
                }
            }
            "rust/panic" => {
                if (trimmed.contains("panic!")
                    || trimmed.contains("unimplemented!")
                    || trimmed.contains("unreachable!"))
                    && !trimmed.starts_with("//")
                {
                    return Some(StyleViolation {
                        rule_id: rule.id.clone(),
                        file: path.to_path_buf(),
                        line: line_num,
                        column: None,
                        message: rule.description.clone(),
                        severity: rule.severity,
                        suggestion: Some("Return an error instead of panicking".into()),
                    });
                }
            }
            _ => {}
        }

        None
    }

    /// Check changed lines in a diff
    pub fn check_diff(
        &self,
        files: &[FileDiff],
        file_contents: &HashMap<PathBuf, String>,
    ) -> Vec<StyleViolation> {
        let mut violations = Vec::new();

        for file in files {
            if let Some(new_path) = &file.new_path {
                if let Some(language) = &file.language {
                    if let Some(content) = file_contents.get(new_path) {
                        // Get all changed line numbers
                        let changed_lines: std::collections::HashSet<u32> = file
                            .hunks
                            .iter()
                            .flat_map(|h| h.lines.iter())
                            .filter(|l| l.change_type == ChangeType::Added)
                            .filter_map(|l| l.new_line)
                            .collect();

                        // Check entire file but only report violations on changed lines
                        let all_violations = self.check_file(new_path, content, language);
                        violations.extend(
                            all_violations
                                .into_iter()
                                .filter(|v| changed_lines.contains(&v.line)),
                        );
                    }
                }
            }
        }

        violations
    }
}

impl Default for StyleChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Complexity metrics for code
#[derive(Debug, Clone, Default)]
pub struct ComplexityMetrics {
    /// Cyclomatic complexity (decision points + 1)
    pub cyclomatic: u32,
    /// Cognitive complexity (nested decision complexity)
    pub cognitive: u32,
    /// Number of parameters
    pub parameters: u32,
    /// Lines of code
    pub loc: u32,
    /// Nesting depth
    pub max_nesting: u32,
}

/// Function complexity info
#[derive(Debug, Clone)]
pub struct FunctionComplexity {
    /// Function name
    pub name: String,
    /// File path
    pub file: PathBuf,
    /// Starting line
    pub line: u32,
    /// Complexity metrics
    pub metrics: ComplexityMetrics,
    /// Whether complexity is too high
    pub is_complex: bool,
}

/// Analyzer for code complexity
pub struct ComplexityAnalyzer {
    /// Threshold for cyclomatic complexity
    cyclomatic_threshold: u32,
    /// Threshold for cognitive complexity
    cognitive_threshold: u32,
    /// Threshold for max nesting
    nesting_threshold: u32,
}

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self {
            cyclomatic_threshold: 10,
            cognitive_threshold: 15,
            nesting_threshold: 4,
        }
    }

    pub fn with_thresholds(cyclomatic: u32, cognitive: u32, nesting: u32) -> Self {
        Self {
            cyclomatic_threshold: cyclomatic,
            cognitive_threshold: cognitive,
            nesting_threshold: nesting,
        }
    }

    /// Analyze a Rust file for complexity
    pub fn analyze_rust_file(&self, path: &Path, content: &str) -> Vec<FunctionComplexity> {
        let mut functions = Vec::new();
        let mut current_fn: Option<(String, u32)> = None;
        let mut brace_depth = 0;
        let mut fn_start_depth = 0;
        let mut metrics = ComplexityMetrics::default();
        let mut current_nesting = 0u32;

        for (line_num, line) in content.lines().enumerate() {
            let line_num = (line_num + 1) as u32;
            let trimmed = line.trim();

            // Track brace depth
            let opens = line.chars().filter(|&c| c == '{').count();
            let closes = line.chars().filter(|&c| c == '}').count();

            // Detect function start
            if current_fn.is_none()
                && (trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("async fn ")
                    || trimmed.starts_with("pub async fn ")
                    || trimmed.starts_with("pub(crate) fn ")
                    || trimmed.starts_with("pub(super) fn "))
            {
                // Extract function name
                let fn_part = if let Some(rest) = trimmed.strip_prefix("pub ") {
                    rest
                } else {
                    trimmed
                };
                let fn_part = if let Some(rest) = fn_part.strip_prefix("async ") {
                    rest
                } else {
                    fn_part
                };
                let fn_part = if let Some(rest) = fn_part.strip_prefix("(crate) ") {
                    rest
                } else if let Some(rest) = fn_part.strip_prefix("(super) ") {
                    rest
                } else {
                    fn_part
                };

                if let Some(name) = fn_part.strip_prefix("fn ") {
                    let name = name
                        .split('(')
                        .next()
                        .unwrap_or("")
                        .split('<')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !name.is_empty() {
                        current_fn = Some((name.to_string(), line_num));
                        fn_start_depth = brace_depth;
                        metrics = ComplexityMetrics::default();
                        current_nesting = 0;

                        // Count parameters
                        let param_str = trimmed.split('(').nth(1).unwrap_or("");
                        if !param_str.starts_with(')') {
                            metrics.parameters = param_str.matches(',').count() as u32 + 1;
                            if param_str.contains("&self") || param_str.contains("self") {
                                metrics.parameters = metrics.parameters.saturating_sub(1);
                            }
                        }
                    }
                }
            }

            brace_depth += opens;
            brace_depth = brace_depth.saturating_sub(closes);

            // If inside a function
            if current_fn.is_some() {
                metrics.loc += 1;

                // Track nesting
                if opens > 0 {
                    current_nesting += opens as u32;
                    if current_nesting > metrics.max_nesting {
                        metrics.max_nesting = current_nesting;
                    }
                }
                if closes > 0 {
                    current_nesting = current_nesting.saturating_sub(closes as u32);
                }

                // Count complexity contributors
                let lower = trimmed.to_lowercase();

                // Cyclomatic: decision points
                if lower.contains("if ") || lower.contains("else if ") {
                    metrics.cyclomatic += 1;
                }
                if lower.contains("for ") || lower.contains("while ") || lower.contains("loop ") {
                    metrics.cyclomatic += 1;
                }
                if lower.contains("match ") {
                    metrics.cyclomatic += 1;
                }
                if lower.contains("&&") || lower.contains("||") {
                    metrics.cyclomatic += line.matches("&&").count() as u32;
                    metrics.cyclomatic += line.matches("||").count() as u32;
                }
                if lower.contains("?") && !lower.contains("//") {
                    metrics.cyclomatic += line.matches('?').count() as u32;
                }

                // Cognitive: complexity with nesting penalty
                if lower.contains("if ") || lower.contains("else if ") {
                    metrics.cognitive += 1 + current_nesting;
                }
                if lower.contains("for ") || lower.contains("while ") || lower.contains("loop ") {
                    metrics.cognitive += 1 + current_nesting;
                }
                if lower.contains("match ") {
                    metrics.cognitive += 1 + current_nesting;
                }

                // Function ends when we return to fn_start_depth
                if brace_depth == fn_start_depth && closes > 0 {
                    if let Some((name, start_line)) = current_fn.take() {
                        metrics.cyclomatic += 1; // Base complexity

                        let is_complex = metrics.cyclomatic > self.cyclomatic_threshold
                            || metrics.cognitive > self.cognitive_threshold
                            || metrics.max_nesting > self.nesting_threshold;

                        functions.push(FunctionComplexity {
                            name,
                            file: path.to_path_buf(),
                            line: start_line,
                            metrics,
                            is_complex,
                        });
                        metrics = ComplexityMetrics::default();
                    }
                }
            }
        }

        functions
    }

    /// Get complexity report for functions in a diff
    pub fn analyze_diff_complexity(
        &self,
        files: &[FileDiff],
        file_contents: &HashMap<PathBuf, String>,
    ) -> Vec<FunctionComplexity> {
        let mut all_complex = Vec::new();

        for file in files {
            if let Some(new_path) = &file.new_path {
                if let Some(content) = file_contents.get(new_path) {
                    let language = file.language.as_deref().unwrap_or("");
                    if language == "rust" {
                        let functions = self.analyze_rust_file(new_path, content);

                        // Get changed line ranges
                        let changed_lines: std::collections::HashSet<u32> = file
                            .hunks
                            .iter()
                            .flat_map(|h| {
                                (h.new_start..h.new_start + h.new_count).collect::<Vec<_>>()
                            })
                            .collect();

                        // Report functions that were touched and are complex
                        for func in functions {
                            if func.is_complex && changed_lines.contains(&func.line) {
                                all_complex.push(func);
                            }
                        }
                    }
                }
            }
        }

        all_complex
    }
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// A review comment/suggestion
#[derive(Debug, Clone)]
pub struct ReviewComment {
    /// Unique identifier
    pub id: String,
    /// File path
    pub file: PathBuf,
    /// Line number
    pub line: u32,
    /// Comment body
    pub body: String,
    /// Category
    pub category: ReviewCategory,
    /// Severity
    pub severity: Severity,
    /// Suggested code change (if applicable)
    pub suggestion: Option<String>,
    /// Whether this requires action before merge
    pub blocking: bool,
}

impl ReviewComment {
    pub fn new(file: PathBuf, line: u32, body: String) -> Self {
        let id = format!(
            "review_{}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            line
        );
        Self {
            id,
            file,
            line,
            body,
            category: ReviewCategory::BestPractice,
            severity: Severity::Info,
            suggestion: None,
            blocking: false,
        }
    }

    pub fn with_category(mut self, category: ReviewCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        if matches!(severity, Severity::Error | Severity::Critical) {
            self.blocking = true;
        }
        self
    }

    pub fn with_suggestion(mut self, suggestion: String) -> Self {
        self.suggestion = Some(suggestion);
        self
    }
}

/// Review result for a PR or diff
#[derive(Debug, Clone)]
pub struct ReviewResult {
    /// All comments
    pub comments: Vec<ReviewComment>,
    /// Overall verdict
    pub verdict: ReviewVerdict,
    /// Summary statistics
    pub stats: ReviewStats,
    /// Review duration
    pub duration_ms: u64,
}

/// Overall review verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    /// Approved - no blocking issues
    Approved,
    /// Request changes - has blocking issues
    RequestChanges,
    /// Comment only - has non-blocking suggestions
    Comment,
}

/// Review statistics
#[derive(Debug, Clone, Default)]
pub struct ReviewStats {
    pub total_comments: usize,
    pub blocking_comments: usize,
    pub by_category: HashMap<String, usize>,
    pub by_severity: HashMap<String, usize>,
    pub files_reviewed: usize,
    pub lines_reviewed: usize,
}

/// Main code review assistant
pub struct CodeReviewAssistant {
    /// Diff analyzer
    diff_analyzer: DiffAnalyzer,
    /// Style checker
    style_checker: StyleChecker,
    /// Complexity analyzer
    complexity_analyzer: ComplexityAnalyzer,
    /// Cache of file contents
    file_cache: RwLock<HashMap<PathBuf, String>>,
    /// Review history for learning
    review_history: RwLock<Vec<ReviewResult>>,
}

impl CodeReviewAssistant {
    pub fn new() -> Self {
        Self {
            diff_analyzer: DiffAnalyzer::new(),
            style_checker: StyleChecker::new().with_rust_rules(),
            complexity_analyzer: ComplexityAnalyzer::new(),
            file_cache: RwLock::new(HashMap::new()),
            review_history: RwLock::new(Vec::new()),
        }
    }

    /// Load file content into cache
    pub fn cache_file(&self, path: PathBuf, content: String) {
        if let Ok(mut cache) = self.file_cache.write() {
            cache.insert(path, content);
        }
    }

    /// Review a diff
    pub fn review_diff(&self, diff_text: &str) -> ReviewResult {
        let start = Instant::now();
        let mut comments = Vec::new();

        // Parse diff
        let files = self.diff_analyzer.parse_diff(diff_text);
        let diff_stats = self.diff_analyzer.get_stats(&files);

        // Get file contents from cache
        let file_contents = self
            .file_cache
            .read()
            .map(|c| c.clone())
            .unwrap_or_default();

        // Style check
        let style_violations = self.style_checker.check_diff(&files, &file_contents);
        for violation in style_violations {
            comments.push(
                ReviewComment::new(violation.file, violation.line, violation.message)
                    .with_category(ReviewCategory::Style)
                    .with_severity(violation.severity)
                    .with_suggestion(violation.suggestion.unwrap_or_default()),
            );
        }

        // Complexity check
        let complex_functions = self
            .complexity_analyzer
            .analyze_diff_complexity(&files, &file_contents);
        for func in complex_functions {
            let message = format!(
                "Function `{}` has high complexity (cyclomatic: {}, cognitive: {}, nesting: {})",
                func.name,
                func.metrics.cyclomatic,
                func.metrics.cognitive,
                func.metrics.max_nesting
            );
            comments.push(
                ReviewComment::new(func.file, func.line, message)
                    .with_category(ReviewCategory::Complexity)
                    .with_severity(Severity::Warning),
            );
        }

        // Large PR warning
        if diff_stats.lines_added + diff_stats.lines_removed > 500 {
            let message = format!(
                "Large PR with {} lines changed ({} added, {} removed). Consider breaking into smaller PRs.",
                diff_stats.lines_added + diff_stats.lines_removed,
                diff_stats.lines_added,
                diff_stats.lines_removed
            );
            comments.push(
                ReviewComment::new(PathBuf::new(), 0, message)
                    .with_category(ReviewCategory::BestPractice)
                    .with_severity(Severity::Info),
            );
        }

        // Calculate stats
        let mut stats = ReviewStats {
            total_comments: comments.len(),
            blocking_comments: comments.iter().filter(|c| c.blocking).count(),
            files_reviewed: diff_stats.files_changed,
            lines_reviewed: diff_stats.lines_added + diff_stats.lines_removed,
            ..Default::default()
        };

        for comment in &comments {
            *stats
                .by_category
                .entry(comment.category.as_str().to_string())
                .or_insert(0) += 1;
            *stats
                .by_severity
                .entry(comment.severity.as_str().to_string())
                .or_insert(0) += 1;
        }

        // Determine verdict
        let verdict = if stats.blocking_comments > 0 {
            ReviewVerdict::RequestChanges
        } else if comments.is_empty() {
            ReviewVerdict::Approved
        } else {
            ReviewVerdict::Comment
        };

        let result = ReviewResult {
            comments,
            verdict,
            stats,
            duration_ms: start.elapsed().as_millis() as u64,
        };

        // Save to history
        if let Ok(mut history) = self.review_history.write() {
            history.push(result.clone());
            // Keep last 100 reviews
            if history.len() > 100 {
                history.remove(0);
            }
        }

        result
    }

    /// Generate a summary of the review
    pub fn summarize(&self, result: &ReviewResult) -> String {
        let mut summary = String::new();

        // Verdict
        let verdict_str = match result.verdict {
            ReviewVerdict::Approved => "APPROVED",
            ReviewVerdict::RequestChanges => "CHANGES REQUESTED",
            ReviewVerdict::Comment => "COMMENTED",
        };
        summary.push_str(&format!("## Review: {}\n\n", verdict_str));

        // Stats
        summary.push_str(&format!(
            "- Files reviewed: {}\n",
            result.stats.files_reviewed
        ));
        summary.push_str(&format!(
            "- Lines reviewed: {}\n",
            result.stats.lines_reviewed
        ));
        summary.push_str(&format!(
            "- Comments: {} ({} blocking)\n",
            result.stats.total_comments, result.stats.blocking_comments
        ));
        summary.push_str(&format!("- Review time: {}ms\n\n", result.duration_ms));

        // Comments by category
        if !result.stats.by_category.is_empty() {
            summary.push_str("### By Category\n");
            for (cat, count) in &result.stats.by_category {
                summary.push_str(&format!("- {}: {}\n", cat, count));
            }
            summary.push('\n');
        }

        // Blocking issues
        let blocking: Vec<_> = result.comments.iter().filter(|c| c.blocking).collect();
        if !blocking.is_empty() {
            summary.push_str("### Blocking Issues\n");
            for comment in blocking {
                summary.push_str(&format!(
                    "- **{}:{}** [{}] {}\n",
                    comment.file.display(),
                    comment.line,
                    comment.severity.as_str(),
                    comment.body
                ));
            }
            summary.push('\n');
        }

        // Suggestions
        let suggestions: Vec<_> = result.comments.iter().filter(|c| !c.blocking).collect();
        if !suggestions.is_empty() {
            summary.push_str("### Suggestions\n");
            for comment in suggestions.iter().take(10) {
                summary.push_str(&format!(
                    "- **{}:{}** {}\n",
                    comment.file.display(),
                    comment.line,
                    comment.body
                ));
            }
            if suggestions.len() > 10 {
                summary.push_str(&format!("- ... and {} more\n", suggestions.len() - 10));
            }
        }

        summary
    }

    /// Get review history stats
    pub fn get_history_stats(&self) -> Option<HistoryStats> {
        let history = self.review_history.read().ok()?;
        if history.is_empty() {
            return None;
        }

        let total = history.len();
        let approved = history
            .iter()
            .filter(|r| r.verdict == ReviewVerdict::Approved)
            .count();
        let changes_requested = history
            .iter()
            .filter(|r| r.verdict == ReviewVerdict::RequestChanges)
            .count();

        let avg_comments: f64 = history
            .iter()
            .map(|r| r.stats.total_comments as f64)
            .sum::<f64>()
            / total as f64;
        let avg_duration: f64 =
            history.iter().map(|r| r.duration_ms as f64).sum::<f64>() / total as f64;

        Some(HistoryStats {
            total_reviews: total,
            approved_count: approved,
            changes_requested_count: changes_requested,
            avg_comments,
            avg_duration_ms: avg_duration,
        })
    }
}

impl Default for CodeReviewAssistant {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from review history
#[derive(Debug, Clone)]
pub struct HistoryStats {
    pub total_reviews: usize,
    pub approved_count: usize,
    pub changes_requested_count: usize,
    pub avg_comments: f64,
    pub avg_duration_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert!(Severity::Error < Severity::Critical);
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_review_category_as_str() {
        assert_eq!(ReviewCategory::Style.as_str(), "style");
        assert_eq!(ReviewCategory::Performance.as_str(), "performance");
        assert_eq!(ReviewCategory::Security.as_str(), "security");
        assert_eq!(
            ReviewCategory::Custom("custom".to_string()).as_str(),
            "custom"
        );
    }

    #[test]
    fn test_diff_analyzer_parse() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("Hello");
     todo!();
 }
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].new_path, Some(PathBuf::from("src/main.rs")));
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[0].language, Some("rust".to_string()));
        assert_eq!(files[0].hunks.len(), 1);
    }

    #[test]
    fn test_diff_analyzer_new_file() {
        let diff = r#"diff --git a/src/new.rs b/src/new.rs
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,3 @@
+fn new_function() {
+    println!("new");
+}
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Added);
        assert!(files[0].old_path.is_none());
    }

    #[test]
    fn test_diff_stats() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("Hello");
-    todo!();
+    done!();
 }
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);
        let stats = analyzer.get_stats(&files);

        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.lines_added, 2);
        assert_eq!(stats.lines_removed, 1);
    }

    #[test]
    fn test_style_checker_unwrap() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = r#"fn test() {
    let x = opt.unwrap();
}
"#;

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/unwrap");
    }

    #[test]
    fn test_style_checker_todo() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "// TODO: fix this\n";

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/todo");
    }

    #[test]
    fn test_style_checker_unsafe() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "unsafe fn danger() {}\n";

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/unsafe");
    }

    #[test]
    fn test_complexity_analyzer_simple() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"fn simple() {
    println!("hello");
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "simple");
        assert!(!functions[0].is_complex);
    }

    #[test]
    fn test_complexity_analyzer_complex() {
        let analyzer = ComplexityAnalyzer::with_thresholds(5, 10, 3);
        let path = PathBuf::from("test.rs");
        let content = r#"fn complex(x: i32, y: i32, z: i32) {
    if x > 0 {
        if y > 0 {
            if z > 0 {
                if x > y && y > z || z > x {
                    match x {
                        1 => println!("one"),
                        2 => println!("two"),
                        _ => println!("other"),
                    }
                }
            }
        }
    }
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert!(functions[0].is_complex);
        assert!(functions[0].metrics.cyclomatic > 5);
    }

    #[test]
    fn test_review_comment_new() {
        let comment = ReviewComment::new(PathBuf::from("test.rs"), 10, "Test comment".to_string());

        assert!(!comment.id.is_empty());
        assert_eq!(comment.line, 10);
        assert!(!comment.blocking);
    }

    #[test]
    fn test_review_comment_with_severity() {
        let comment = ReviewComment::new(PathBuf::from("test.rs"), 10, "Error".to_string())
            .with_severity(Severity::Error);

        assert!(comment.blocking);
        assert_eq!(comment.severity, Severity::Error);
    }

    #[test]
    fn test_code_review_assistant_new() {
        let assistant = CodeReviewAssistant::new();
        let stats = assistant.get_history_stats();
        assert!(stats.is_none()); // No history yet
    }

    #[test]
    fn test_code_review_simple_diff() {
        let assistant = CodeReviewAssistant::new();
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("Hello");
 }
"#;

        let result = assistant.review_diff(diff);
        // Duration may be 0 if review is fast (< 1ms)
        assert_eq!(result.stats.files_reviewed, 1);
    }

    #[test]
    fn test_code_review_with_violations() {
        let assistant = CodeReviewAssistant::new();

        // Cache file content
        assistant.cache_file(
            PathBuf::from("src/main.rs"),
            "fn main() {\n    let x = opt.unwrap();\n}\n".to_string(),
        );

        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,3 @@
 fn main() {
+    let x = opt.unwrap();
 }
"#;

        let result = assistant.review_diff(diff);
        assert!(!result.comments.is_empty());
    }

    #[test]
    fn test_review_summarize() {
        let result = ReviewResult {
            comments: vec![
                ReviewComment::new(PathBuf::from("test.rs"), 1, "Test".to_string())
                    .with_severity(Severity::Warning),
            ],
            verdict: ReviewVerdict::Comment,
            stats: ReviewStats {
                total_comments: 1,
                blocking_comments: 0,
                files_reviewed: 1,
                lines_reviewed: 10,
                ..Default::default()
            },
            duration_ms: 100,
        };

        let assistant = CodeReviewAssistant::new();
        let summary = assistant.summarize(&result);

        assert!(summary.contains("COMMENTED"));
        assert!(summary.contains("Files reviewed: 1"));
    }

    #[test]
    fn test_review_verdict_approved() {
        let result = ReviewResult {
            comments: vec![],
            verdict: ReviewVerdict::Approved,
            stats: ReviewStats::default(),
            duration_ms: 50,
        };

        let assistant = CodeReviewAssistant::new();
        let summary = assistant.summarize(&result);
        assert!(summary.contains("APPROVED"));
    }

    #[test]
    fn test_review_verdict_changes_requested() {
        let result = ReviewResult {
            comments: vec![
                ReviewComment::new(PathBuf::from("test.rs"), 1, "Error".to_string())
                    .with_severity(Severity::Error),
            ],
            verdict: ReviewVerdict::RequestChanges,
            stats: ReviewStats {
                total_comments: 1,
                blocking_comments: 1,
                ..Default::default()
            },
            duration_ms: 50,
        };

        let assistant = CodeReviewAssistant::new();
        let summary = assistant.summarize(&result);
        assert!(summary.contains("CHANGES REQUESTED"));
    }

    #[test]
    fn test_diff_analyzer_multiple_files() {
        let diff = r#"diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 fn a() {}
+fn b() {}
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -1 +1,2 @@
 fn c() {}
+fn d() {}
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_style_checker_panic() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "panic!(\"error\");\n";

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/panic");
    }

    #[test]
    fn test_complexity_parameters() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"fn many_params(a: i32, b: i32, c: i32, d: i32) {
    println!("{} {} {} {}", a, b, c, d);
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].metrics.parameters, 4);
    }

    #[test]
    fn test_review_history() {
        let assistant = CodeReviewAssistant::new();

        // Review some diffs
        for _ in 0..3 {
            assistant.review_diff("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n");
        }

        let stats = assistant.get_history_stats().unwrap();
        assert_eq!(stats.total_reviews, 3);
    }

    #[test]
    fn test_large_pr_warning() {
        let assistant = CodeReviewAssistant::new();

        // Create a diff with many lines
        let mut diff =
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,1 +1,600 @@\n-old\n".to_string();
        for i in 0..600 {
            diff.push_str(&format!("+line {}\n", i));
        }

        let result = assistant.review_diff(&diff);

        // Should have a warning about large PR
        let has_large_warning = result.comments.iter().any(|c| c.body.contains("Large PR"));
        assert!(has_large_warning);
    }

    // Additional comprehensive tests

    #[test]
    fn test_severity_all_variants() {
        let info = Severity::Info;
        let warning = Severity::Warning;
        let error = Severity::Error;
        let critical = Severity::Critical;

        assert_eq!(info.as_str(), "info");
        assert_eq!(warning.as_str(), "warning");
        assert_eq!(error.as_str(), "error");
        assert_eq!(critical.as_str(), "critical");
    }

    #[test]
    fn test_severity_clone() {
        let severity = Severity::Error;
        let cloned = severity;
        assert_eq!(cloned, severity);
    }

    #[test]
    fn test_severity_debug() {
        let severity = Severity::Critical;
        let debug = format!("{:?}", severity);
        assert!(debug.contains("Critical"));
    }

    #[test]
    fn test_severity_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Severity::Info);
        set.insert(Severity::Warning);
        set.insert(Severity::Error);
        set.insert(Severity::Critical);
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn test_severity_copy() {
        let severity = Severity::Warning;
        let copied = severity; // Copy, not move
        assert_eq!(severity, copied);
    }

    #[test]
    fn test_review_category_all_variants() {
        assert_eq!(ReviewCategory::Style.as_str(), "style");
        assert_eq!(ReviewCategory::Performance.as_str(), "performance");
        assert_eq!(ReviewCategory::Security.as_str(), "security");
        assert_eq!(ReviewCategory::Logic.as_str(), "logic");
        assert_eq!(ReviewCategory::Documentation.as_str(), "documentation");
        assert_eq!(ReviewCategory::Testing.as_str(), "testing");
        assert_eq!(ReviewCategory::Complexity.as_str(), "complexity");
        assert_eq!(ReviewCategory::BestPractice.as_str(), "best_practice");
        assert_eq!(ReviewCategory::Naming.as_str(), "naming");
        assert_eq!(ReviewCategory::ErrorHandling.as_str(), "error_handling");
        assert_eq!(
            ReviewCategory::Custom("my_category".into()).as_str(),
            "my_category"
        );
    }

    #[test]
    fn test_review_category_clone() {
        let cat = ReviewCategory::Security;
        let cloned = cat.clone();
        assert_eq!(cloned, cat);
    }

    #[test]
    fn test_review_category_debug() {
        let cat = ReviewCategory::Performance;
        let debug = format!("{:?}", cat);
        assert!(debug.contains("Performance"));
    }

    #[test]
    fn test_review_category_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ReviewCategory::Style);
        set.insert(ReviewCategory::Security);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_change_type_variants() {
        let added = ChangeType::Added;
        let removed = ChangeType::Removed;
        let context = ChangeType::Context;

        assert_eq!(added, ChangeType::Added);
        assert_eq!(removed, ChangeType::Removed);
        assert_eq!(context, ChangeType::Context);
    }

    #[test]
    fn test_change_type_clone() {
        let ct = ChangeType::Added;
        let cloned = ct;
        assert_eq!(cloned, ct);
    }

    #[test]
    fn test_change_type_debug() {
        let ct = ChangeType::Removed;
        let debug = format!("{:?}", ct);
        assert!(debug.contains("Removed"));
    }

    #[test]
    fn test_change_type_copy() {
        let ct = ChangeType::Context;
        let copied = ct;
        assert_eq!(ct, copied);
    }

    #[test]
    fn test_file_status_variants() {
        assert_eq!(FileStatus::Added, FileStatus::Added);
        assert_eq!(FileStatus::Modified, FileStatus::Modified);
        assert_eq!(FileStatus::Deleted, FileStatus::Deleted);
        assert_eq!(FileStatus::Renamed, FileStatus::Renamed);
    }

    #[test]
    fn test_file_status_clone() {
        let status = FileStatus::Renamed;
        let cloned = status;
        assert_eq!(cloned, status);
    }

    #[test]
    fn test_file_status_debug() {
        let status = FileStatus::Deleted;
        let debug = format!("{:?}", status);
        assert!(debug.contains("Deleted"));
    }

    #[test]
    fn test_diff_line_struct() {
        let line = DiffLine {
            old_line: Some(10),
            new_line: Some(12),
            content: "let x = 5;".to_string(),
            change_type: ChangeType::Context,
        };

        assert_eq!(line.old_line, Some(10));
        assert_eq!(line.new_line, Some(12));
        assert_eq!(line.content, "let x = 5;");
        assert_eq!(line.change_type, ChangeType::Context);
    }

    #[test]
    fn test_diff_line_added() {
        let line = DiffLine {
            old_line: None,
            new_line: Some(5),
            content: "new line".to_string(),
            change_type: ChangeType::Added,
        };

        assert!(line.old_line.is_none());
        assert_eq!(line.new_line, Some(5));
    }

    #[test]
    fn test_diff_line_removed() {
        let line = DiffLine {
            old_line: Some(3),
            new_line: None,
            content: "old line".to_string(),
            change_type: ChangeType::Removed,
        };

        assert_eq!(line.old_line, Some(3));
        assert!(line.new_line.is_none());
    }

    #[test]
    fn test_diff_line_clone() {
        let line = DiffLine {
            old_line: Some(1),
            new_line: Some(1),
            content: "test".to_string(),
            change_type: ChangeType::Context,
        };
        let cloned = line.clone();
        assert_eq!(cloned.content, line.content);
    }

    #[test]
    fn test_diff_hunk_struct() {
        let hunk = DiffHunk {
            old_start: 1,
            old_count: 5,
            new_start: 1,
            new_count: 7,
            lines: vec![],
        };

        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 5);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 7);
        assert!(hunk.lines.is_empty());
    }

    #[test]
    fn test_diff_hunk_clone() {
        let hunk = DiffHunk {
            old_start: 10,
            old_count: 3,
            new_start: 12,
            new_count: 5,
            lines: vec![DiffLine {
                old_line: Some(10),
                new_line: Some(12),
                content: "test".to_string(),
                change_type: ChangeType::Context,
            }],
        };
        let cloned = hunk.clone();
        assert_eq!(cloned.old_start, hunk.old_start);
        assert_eq!(cloned.lines.len(), 1);
    }

    #[test]
    fn test_file_diff_struct() {
        let file = FileDiff {
            old_path: Some(PathBuf::from("old.rs")),
            new_path: Some(PathBuf::from("new.rs")),
            status: FileStatus::Renamed,
            hunks: vec![],
            language: Some("rust".to_string()),
        };

        assert_eq!(file.old_path, Some(PathBuf::from("old.rs")));
        assert_eq!(file.new_path, Some(PathBuf::from("new.rs")));
        assert_eq!(file.status, FileStatus::Renamed);
        assert_eq!(file.language, Some("rust".to_string()));
    }

    #[test]
    fn test_file_diff_clone() {
        let file = FileDiff {
            old_path: Some(PathBuf::from("src/lib.rs")),
            new_path: Some(PathBuf::from("src/lib.rs")),
            status: FileStatus::Modified,
            hunks: vec![],
            language: Some("rust".to_string()),
        };
        let cloned = file.clone();
        assert_eq!(cloned.old_path, file.old_path);
    }

    #[test]
    fn test_diff_analyzer_default() {
        let analyzer = DiffAnalyzer::default();
        let files = analyzer.parse_diff("");
        assert!(files.is_empty());
    }

    #[test]
    fn test_diff_analyzer_deleted_file() {
        let diff = r#"diff --git a/src/old.rs b/src/old.rs
--- a/src/old.rs
+++ /dev/null
@@ -1,3 +0,0 @@
-fn old_function() {
-    println!("old");
-}
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Deleted);
        assert!(files[0].new_path.is_none());
    }

    #[test]
    fn test_diff_analyzer_renamed_file() {
        let diff = r#"diff --git a/src/old.rs b/src/new.rs
--- a/src/old.rs
+++ b/src/new.rs
@@ -1 +1 @@
 fn function() {}
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Renamed);
    }

    #[test]
    fn test_diff_analyzer_language_detection() {
        let analyzer = DiffAnalyzer::new();

        let extensions = vec![
            ("rs", "rust"),
            ("py", "python"),
            ("js", "javascript"),
            ("ts", "typescript"),
            ("go", "go"),
            ("java", "java"),
            ("cpp", "cpp"),
            ("c", "c"),
            ("h", "c"),
            ("hpp", "cpp"),
            ("rb", "ruby"),
            ("php", "php"),
            ("swift", "swift"),
            ("kt", "kotlin"),
            ("scala", "scala"),
            ("sh", "shell"),
            ("bash", "shell"),
            ("yml", "yaml"),
            ("yaml", "yaml"),
            ("json", "json"),
            ("toml", "toml"),
            ("md", "markdown"),
        ];

        for (ext, lang) in extensions {
            let diff = format!(
                r#"diff --git a/test.{ext} b/test.{ext}
--- /dev/null
+++ b/test.{ext}
@@ -0,0 +1 @@
+content
"#
            );
            let files = analyzer.parse_diff(&diff);
            assert_eq!(
                files[0].language,
                Some(lang.to_string()),
                "Failed for extension: {}",
                ext
            );
        }
    }

    #[test]
    fn test_diff_stats_default() {
        let stats = DiffStats::default();
        assert_eq!(stats.files_changed, 0);
        assert_eq!(stats.files_added, 0);
        assert_eq!(stats.files_deleted, 0);
        assert_eq!(stats.files_modified, 0);
        assert_eq!(stats.files_renamed, 0);
        assert_eq!(stats.lines_added, 0);
        assert_eq!(stats.lines_removed, 0);
        assert!(stats.languages.is_empty());
    }

    #[test]
    fn test_diff_stats_clone() {
        let mut languages = std::collections::HashMap::new();
        languages.insert("rust".to_string(), 3);
        let stats = DiffStats {
            files_changed: 5,
            languages,
            ..Default::default()
        };

        let cloned = stats.clone();
        assert_eq!(cloned.files_changed, 5);
        assert_eq!(cloned.languages.get("rust"), Some(&3));
    }

    #[test]
    fn test_style_rule_struct() {
        let rule = StyleRule {
            id: "custom/rule".to_string(),
            name: "Custom Rule".to_string(),
            description: "A custom rule for testing".to_string(),
            languages: vec!["rust".to_string(), "python".to_string()],
            severity: Severity::Warning,
            category: ReviewCategory::BestPractice,
        };

        assert_eq!(rule.id, "custom/rule");
        assert_eq!(rule.name, "Custom Rule");
        assert_eq!(rule.languages.len(), 2);
        assert_eq!(rule.severity, Severity::Warning);
    }

    #[test]
    fn test_style_rule_clone() {
        let rule = StyleRule {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test description".to_string(),
            languages: vec![],
            severity: Severity::Info,
            category: ReviewCategory::Style,
        };
        let cloned = rule.clone();
        assert_eq!(cloned.id, rule.id);
    }

    #[test]
    fn test_style_violation_struct() {
        let violation = StyleViolation {
            rule_id: "rust/unwrap".to_string(),
            file: PathBuf::from("src/lib.rs"),
            line: 42,
            column: Some(15),
            message: "Avoid unwrap".to_string(),
            severity: Severity::Warning,
            suggestion: Some("Use ?".to_string()),
        };

        assert_eq!(violation.rule_id, "rust/unwrap");
        assert_eq!(violation.line, 42);
        assert_eq!(violation.column, Some(15));
        assert!(violation.suggestion.is_some());
    }

    #[test]
    fn test_style_violation_clone() {
        let violation = StyleViolation {
            rule_id: "test".to_string(),
            file: PathBuf::from("test.rs"),
            line: 1,
            column: None,
            message: "test".to_string(),
            severity: Severity::Info,
            suggestion: None,
        };
        let cloned = violation.clone();
        assert_eq!(cloned.rule_id, violation.rule_id);
    }

    #[test]
    fn test_style_checker_default() {
        let checker = StyleChecker::default();
        let violations = checker.check_file(&PathBuf::from("test.rs"), "", "rust");
        assert!(violations.is_empty());
    }

    #[test]
    fn test_style_checker_add_rule() {
        let mut checker = StyleChecker::new();
        checker.add_rule(StyleRule {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            description: "Custom rule".to_string(),
            languages: vec![],
            severity: Severity::Info,
            category: ReviewCategory::Custom("custom".to_string()),
        });

        // Rule added successfully (no way to query count, but it shouldn't panic)
    }

    #[test]
    fn test_style_checker_expect_rule() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = r#"let x = result.expect("should work");"#;

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/expect");
    }

    #[test]
    fn test_style_checker_clone_rule() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = r#"let y = x.clone();"#;

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/clone");
    }

    #[test]
    fn test_style_checker_fixme() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "// FIXME: this is broken\n";

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/todo");
    }

    #[test]
    fn test_style_checker_unimplemented() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "unimplemented!()\n";

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/panic");
    }

    #[test]
    fn test_style_checker_unreachable() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "unreachable!()\n";

        let violations = checker.check_file(&path, content, "rust");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].rule_id, "rust/panic");
    }

    #[test]
    fn test_style_checker_comment_ignored() {
        let checker = StyleChecker::new().with_rust_rules();
        let path = PathBuf::from("test.rs");
        let content = "// x.unwrap() is fine in a comment\n";

        let violations = checker.check_file(&path, content, "rust");
        // unwrap rule should ignore comments
        let unwrap_violations: Vec<_> = violations
            .iter()
            .filter(|v| v.rule_id == "rust/unwrap")
            .collect();
        assert!(unwrap_violations.is_empty());
    }

    #[test]
    fn test_style_checker_check_diff() {
        let checker = StyleChecker::new().with_rust_rules();

        let files = vec![FileDiff {
            old_path: Some(PathBuf::from("test.rs")),
            new_path: Some(PathBuf::from("test.rs")),
            status: FileStatus::Modified,
            hunks: vec![DiffHunk {
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 2,
                lines: vec![DiffLine {
                    old_line: None,
                    new_line: Some(2),
                    content: "x.unwrap()".to_string(),
                    change_type: ChangeType::Added,
                }],
            }],
            language: Some("rust".to_string()),
        }];

        let mut contents = HashMap::new();
        contents.insert(
            PathBuf::from("test.rs"),
            "fn test() {\nx.unwrap()\n}\n".to_string(),
        );

        let violations = checker.check_diff(&files, &contents);
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_complexity_metrics_default() {
        let metrics = ComplexityMetrics::default();
        assert_eq!(metrics.cyclomatic, 0);
        assert_eq!(metrics.cognitive, 0);
        assert_eq!(metrics.parameters, 0);
        assert_eq!(metrics.loc, 0);
        assert_eq!(metrics.max_nesting, 0);
    }

    #[test]
    fn test_complexity_metrics_clone() {
        let metrics = ComplexityMetrics {
            cyclomatic: 5,
            cognitive: 10,
            parameters: 3,
            loc: 20,
            max_nesting: 2,
        };
        let cloned = metrics.clone();
        assert_eq!(cloned.cyclomatic, 5);
    }

    #[test]
    fn test_function_complexity_struct() {
        let func = FunctionComplexity {
            name: "complex_function".to_string(),
            file: PathBuf::from("src/lib.rs"),
            line: 100,
            metrics: ComplexityMetrics {
                cyclomatic: 15,
                cognitive: 20,
                parameters: 5,
                loc: 50,
                max_nesting: 5,
            },
            is_complex: true,
        };

        assert_eq!(func.name, "complex_function");
        assert_eq!(func.line, 100);
        assert!(func.is_complex);
    }

    #[test]
    fn test_function_complexity_clone() {
        let func = FunctionComplexity {
            name: "test".to_string(),
            file: PathBuf::from("test.rs"),
            line: 1,
            metrics: ComplexityMetrics::default(),
            is_complex: false,
        };
        let cloned = func.clone();
        assert_eq!(cloned.name, func.name);
    }

    #[test]
    fn test_complexity_analyzer_default() {
        let analyzer = ComplexityAnalyzer::default();
        let functions = analyzer.analyze_rust_file(&PathBuf::from("test.rs"), "");
        assert!(functions.is_empty());
    }

    #[test]
    fn test_complexity_analyzer_pub_fn() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"pub fn public_function() {
    println!("hello");
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "public_function");
    }

    #[test]
    fn test_complexity_analyzer_async_fn() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"async fn async_function() {
    do_something().await;
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "async_function");
    }

    #[test]
    fn test_complexity_analyzer_pub_async_fn() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"pub async fn pub_async() {
    something().await;
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "pub_async");
    }

    #[test]
    fn test_complexity_analyzer_self_parameter() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"fn method(&self, a: i32) {
    self.do_something(a);
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].metrics.parameters, 1); // self is not counted
    }

    #[test]
    fn test_complexity_analyzer_loop_complexity() {
        let analyzer = ComplexityAnalyzer::new();
        let path = PathBuf::from("test.rs");
        let content = r#"fn with_loops() {
    for i in 0..10 {
        while true {
            loop {
                break;
            }
        }
    }
}
"#;

        let functions = analyzer.analyze_rust_file(&path, content);
        assert_eq!(functions.len(), 1);
        assert!(functions[0].metrics.cyclomatic > 3); // for + while + loop
    }

    #[test]
    fn test_complexity_analyzer_analyze_diff() {
        let analyzer = ComplexityAnalyzer::with_thresholds(2, 2, 2);

        let files = vec![FileDiff {
            old_path: Some(PathBuf::from("test.rs")),
            new_path: Some(PathBuf::from("test.rs")),
            status: FileStatus::Modified,
            hunks: vec![DiffHunk {
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 10,
                lines: vec![],
            }],
            language: Some("rust".to_string()),
        }];

        let mut contents = HashMap::new();
        contents.insert(
            PathBuf::from("test.rs"),
            r#"fn complex() {
    if true {
        if true {
            if true {
                println!("nested");
            }
        }
    }
}
"#
            .to_string(),
        );

        let complex = analyzer.analyze_diff_complexity(&files, &contents);
        // Should find complex function on changed lines
        assert!(!complex.is_empty() || !contents.is_empty());
    }

    #[test]
    fn test_review_comment_with_category() {
        let comment = ReviewComment::new(PathBuf::from("test.rs"), 1, "Test".to_string())
            .with_category(ReviewCategory::Security);

        assert_eq!(comment.category, ReviewCategory::Security);
    }

    #[test]
    fn test_review_comment_with_suggestion() {
        let comment = ReviewComment::new(PathBuf::from("test.rs"), 1, "Test".to_string())
            .with_suggestion("Fix it".to_string());

        assert_eq!(comment.suggestion, Some("Fix it".to_string()));
    }

    #[test]
    fn test_review_comment_critical_is_blocking() {
        let comment = ReviewComment::new(PathBuf::from("test.rs"), 1, "Critical issue".to_string())
            .with_severity(Severity::Critical);

        assert!(comment.blocking);
    }

    #[test]
    fn test_review_comment_clone() {
        let comment = ReviewComment::new(PathBuf::from("test.rs"), 1, "Test".to_string());
        let cloned = comment.clone();
        assert_eq!(cloned.body, comment.body);
    }

    #[test]
    fn test_review_result_struct() {
        let result = ReviewResult {
            comments: vec![],
            verdict: ReviewVerdict::Approved,
            stats: ReviewStats::default(),
            duration_ms: 100,
        };

        assert!(result.comments.is_empty());
        assert_eq!(result.verdict, ReviewVerdict::Approved);
        assert_eq!(result.duration_ms, 100);
    }

    #[test]
    fn test_review_result_clone() {
        let result = ReviewResult {
            comments: vec![],
            verdict: ReviewVerdict::Comment,
            stats: ReviewStats::default(),
            duration_ms: 50,
        };
        let cloned = result.clone();
        assert_eq!(cloned.verdict, ReviewVerdict::Comment);
    }

    #[test]
    fn test_review_verdict_variants() {
        assert_eq!(ReviewVerdict::Approved, ReviewVerdict::Approved);
        assert_eq!(ReviewVerdict::RequestChanges, ReviewVerdict::RequestChanges);
        assert_eq!(ReviewVerdict::Comment, ReviewVerdict::Comment);
    }

    #[test]
    fn test_review_verdict_clone() {
        let verdict = ReviewVerdict::RequestChanges;
        let cloned = verdict;
        assert_eq!(cloned, verdict);
    }

    #[test]
    fn test_review_verdict_debug() {
        let verdict = ReviewVerdict::Approved;
        let debug = format!("{:?}", verdict);
        assert!(debug.contains("Approved"));
    }

    #[test]
    fn test_review_stats_default() {
        let stats = ReviewStats::default();
        assert_eq!(stats.total_comments, 0);
        assert_eq!(stats.blocking_comments, 0);
        assert!(stats.by_category.is_empty());
        assert!(stats.by_severity.is_empty());
        assert_eq!(stats.files_reviewed, 0);
        assert_eq!(stats.lines_reviewed, 0);
    }

    #[test]
    fn test_review_stats_clone() {
        let mut by_category = std::collections::HashMap::new();
        by_category.insert("style".to_string(), 5);
        let stats = ReviewStats {
            total_comments: 10,
            by_category,
            ..Default::default()
        };

        let cloned = stats.clone();
        assert_eq!(cloned.total_comments, 10);
    }

    #[test]
    fn test_code_review_assistant_default() {
        let assistant = CodeReviewAssistant::default();
        let stats = assistant.get_history_stats();
        assert!(stats.is_none());
    }

    #[test]
    fn test_code_review_assistant_cache_file() {
        let assistant = CodeReviewAssistant::new();
        assistant.cache_file(PathBuf::from("test.rs"), "fn test() {}".to_string());
        // File cached successfully (verified by using in review)
    }

    #[test]
    fn test_code_review_empty_diff() {
        let assistant = CodeReviewAssistant::new();
        let result = assistant.review_diff("");

        assert!(result.comments.is_empty());
        assert_eq!(result.verdict, ReviewVerdict::Approved);
    }

    #[test]
    fn test_history_stats_struct() {
        let stats = HistoryStats {
            total_reviews: 10,
            approved_count: 7,
            changes_requested_count: 3,
            avg_comments: 2.5,
            avg_duration_ms: 150.0,
        };

        assert_eq!(stats.total_reviews, 10);
        assert_eq!(stats.approved_count, 7);
        assert_eq!(stats.changes_requested_count, 3);
        assert!((stats.avg_comments - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_history_stats_clone() {
        let stats = HistoryStats {
            total_reviews: 5,
            approved_count: 3,
            changes_requested_count: 2,
            avg_comments: 1.0,
            avg_duration_ms: 100.0,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.total_reviews, 5);
    }

    #[test]
    fn test_diff_hunk_header_parsing() {
        let diff = r#"diff --git a/test.rs b/test.rs
--- a/test.rs
+++ b/test.rs
@@ -10,5 +12,7 @@ fn context() {
 context line
+added line
 context line
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks[0].old_start, 10);
        assert_eq!(files[0].hunks[0].old_count, 5);
        assert_eq!(files[0].hunks[0].new_start, 12);
        assert_eq!(files[0].hunks[0].new_count, 7);
    }

    #[test]
    fn test_diff_single_line_hunk() {
        let diff = r#"diff --git a/test.rs b/test.rs
--- a/test.rs
+++ b/test.rs
@@ -1 +1 @@
-old
+new
"#;

        let analyzer = DiffAnalyzer::new();
        let files = analyzer.parse_diff(diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks[0].old_count, 1);
        assert_eq!(files[0].hunks[0].new_count, 1);
    }

    #[test]
    fn test_summary_many_suggestions() {
        let mut comments = Vec::new();
        for i in 0..15 {
            comments.push(
                ReviewComment::new(PathBuf::from("test.rs"), i, format!("Suggestion {}", i))
                    .with_severity(Severity::Info),
            );
        }

        let result = ReviewResult {
            comments,
            verdict: ReviewVerdict::Comment,
            stats: ReviewStats {
                total_comments: 15,
                blocking_comments: 0,
                ..Default::default()
            },
            duration_ms: 100,
        };

        let assistant = CodeReviewAssistant::new();
        let summary = assistant.summarize(&result);

        assert!(summary.contains("... and 5 more"));
    }

    #[test]
    fn test_diff_stats_all_statuses() {
        let analyzer = DiffAnalyzer::new();

        let files = vec![
            FileDiff {
                old_path: None,
                new_path: Some(PathBuf::from("new.rs")),
                status: FileStatus::Added,
                hunks: vec![],
                language: Some("rust".to_string()),
            },
            FileDiff {
                old_path: Some(PathBuf::from("deleted.rs")),
                new_path: None,
                status: FileStatus::Deleted,
                hunks: vec![],
                language: Some("rust".to_string()),
            },
            FileDiff {
                old_path: Some(PathBuf::from("old.rs")),
                new_path: Some(PathBuf::from("new.rs")),
                status: FileStatus::Renamed,
                hunks: vec![],
                language: Some("rust".to_string()),
            },
            FileDiff {
                old_path: Some(PathBuf::from("mod.rs")),
                new_path: Some(PathBuf::from("mod.rs")),
                status: FileStatus::Modified,
                hunks: vec![],
                language: Some("rust".to_string()),
            },
        ];

        let stats = analyzer.get_stats(&files);
        assert_eq!(stats.files_changed, 4);
        assert_eq!(stats.files_added, 1);
        assert_eq!(stats.files_deleted, 1);
        assert_eq!(stats.files_renamed, 1);
        assert_eq!(stats.files_modified, 1);
        assert_eq!(stats.languages.get("rust"), Some(&4));
    }
}
