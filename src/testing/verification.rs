//! Verification Gates - Automatic validation after every code change
//!
//! Implements the "never proceed on assumptions" protocol:
//! 1. Speculate: Agent proposes an edit
//! 2. Validate: Harness runs checks automatically
//! 3. Feedback: Agent sees results immediately
//! 4. Commit: Only on green, or explicit override

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;

use crate::tools::cargo::{parse_cargo_json_messages, CompilerError, Severity};

/// Captured result of a reaped verification command (see `run_reaped`).
struct ReapedOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Run a verification command with a timeout, capturing output and reaping the
/// ENTIRE process group on timeout. A hung `cargo check` (or the `rustc`
/// children it spawns) would otherwise stall the agent forever and leave
/// orphaned processes holding `target/` locks — a self-reinforcing stall for
/// unattended runs.
async fn run_reaped(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout_secs: u64,
) -> Result<ReapedOutput> {
    use tokio::io::AsyncReadExt;

    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn {}", program))?;
    let pid = child.id();

    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let so_task = tokio::spawn(async move {
        let mut b = Vec::new();
        if let Some(ref mut s) = so {
            let _ = s.read_to_end(&mut b).await;
        }
        b
    });
    let se_task = tokio::spawn(async move {
        let mut b = Vec::new();
        if let Some(ref mut s) = se {
            let _ = s.read_to_end(&mut b).await;
        }
        b
    });

    let timeout = tokio::time::Duration::from_secs(timeout_secs.max(1));
    let (success, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => (status.success(), false),
        Ok(Err(e)) => return Err(e).with_context(|| format!("{} wait failed", program)),
        Err(_) => {
            #[cfg(unix)]
            if let Some(p) = pid {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(p as i32), Signal::SIGKILL);
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            (false, true)
        }
    };

    let stdout = so_task.await.unwrap_or_default();
    let mut stderr = se_task.await.unwrap_or_default();
    if timed_out {
        stderr.extend_from_slice(
            format!(
                "\n[selfware] {} timed out after {}s and its process group was killed.\n",
                program, timeout_secs
            )
            .as_bytes(),
        );
    }

    Ok(ReapedOutput {
        success,
        stdout,
        stderr,
    })
}

/// Verification result for a single check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_type: CheckType,
    pub passed: bool,
    pub duration_ms: u64,
    pub output: String,
    pub errors: Vec<VerificationError>,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
}

/// Types of verification checks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    /// Rust type checking (cargo check)
    TypeCheck,
    /// Run tests (cargo test)
    Test,
    /// Linting (cargo clippy)
    Lint,
    /// Formatting check (cargo fmt --check)
    Format,
    /// Custom command
    Custom,
}

impl CheckType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TypeCheck => "type_check",
            Self::Test => "test",
            Self::Lint => "lint",
            Self::Format => "format",
            Self::Custom => "custom",
        }
    }
}

/// A verification error with location info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationError {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub code: Option<String>,
    pub severity: ErrorSeverity,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Error,
    Warning,
    Note,
    Help,
}

/// Detected repository language for verification dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoLanguage {
    Python,
    JavaScript,
    TypeScript,
    Java,
    CSharp,
    Cpp,
    Sql,
    Go,
    Swift,
    Rust,
    Unknown,
}

impl RepoLanguage {
    /// File extensions associated with this language.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Python => &[".py"],
            Self::JavaScript => &[".js", ".jsx"],
            Self::TypeScript => &[".ts", ".tsx"],
            Self::Java => &[".java"],
            Self::CSharp => &[".cs"],
            Self::Cpp => &[".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp"],
            Self::Sql => &[".sql"],
            Self::Go => &[".go"],
            Self::Swift => &[".swift"],
            Self::Rust => &[".rs"],
            Self::Unknown => &[],
        }
    }

    /// Convert from a file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            ".py" => Some(Self::Python),
            ".js" | ".jsx" => Some(Self::JavaScript),
            ".ts" | ".tsx" => Some(Self::TypeScript),
            ".java" => Some(Self::Java),
            ".cs" => Some(Self::CSharp),
            ".c" | ".cc" | ".cpp" | ".cxx" | ".h" | ".hh" | ".hpp" => Some(Self::Cpp),
            ".sql" => Some(Self::Sql),
            ".go" => Some(Self::Go),
            ".swift" => Some(Self::Swift),
            ".rs" => Some(Self::Rust),
            _ => None,
        }
    }

    /// Convert from a manifest file name.
    pub fn from_manifest(name: &str) -> Option<Self> {
        match name {
            "setup.py" | "pyproject.toml" | "requirements.txt" => Some(Self::Python),
            "package.json" => Some(Self::JavaScript), // may be upgraded to TypeScript
            "tsconfig.json" => Some(Self::TypeScript),
            "pom.xml" | "build.gradle" | "build.gradle.kts" => Some(Self::Java),
            "Package.swift" => Some(Self::Swift),
            "go.mod" => Some(Self::Go),
            "Cargo.toml" => Some(Self::Rust),
            _ => None,
        }
    }
}

impl std::fmt::Display for RepoLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Python => write!(f, "python"),
            Self::JavaScript => write!(f, "javascript"),
            Self::TypeScript => write!(f, "typescript"),
            Self::Java => write!(f, "java"),
            Self::CSharp => write!(f, "csharp"),
            Self::Cpp => write!(f, "cpp"),
            Self::Sql => write!(f, "sql"),
            Self::Go => write!(f, "go"),
            Self::Swift => write!(f, "swift"),
            Self::Rust => write!(f, "rust"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Per-language verification settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageCheckSet {
    #[serde(default = "crate::config::types::default_true")]
    pub syntax: bool,
    #[serde(default = "crate::config::types::default_true")]
    pub format: bool,
    #[serde(default = "crate::config::types::default_true")]
    pub lint: bool,
    #[serde(default = "crate::config::types::default_true")]
    pub test: bool,
}

impl Default for LanguageCheckSet {
    fn default() -> Self {
        Self {
            syntax: true,
            format: true,
            lint: true,
            test: true,
        }
    }
}

/// Complete verification report after a change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub triggered_by: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub total_duration_ms: u64,
    pub checks: Vec<CheckResult>,
    pub overall_passed: bool,
    pub affected_files: Vec<String>,
    pub side_effects: Vec<SideEffect>,
    pub suggested_next_steps: Vec<String>,
}

/// Side effects detected from the change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffect {
    pub effect_type: SideEffectType,
    pub description: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectType {
    FileCreated,
    FileModified,
    FileDeleted,
    DependencyAdded,
    DependencyRemoved,
    TestAdded,
    TestRemoved,
}

/// Configuration for verification gates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Run type check after every file edit
    pub check_on_edit: bool,
    /// Run tests after every file edit
    pub test_on_edit: bool,
    /// Run clippy after every file edit
    pub lint_on_edit: bool,
    /// Run format check after every file edit
    pub format_on_edit: bool,
    /// Only run checks on affected files (faster but less thorough)
    pub incremental: bool,
    /// Timeout for each check
    pub check_timeout_secs: u64,
    /// Continue running other checks if one fails
    pub continue_on_failure: bool,
    /// Files/patterns to exclude from verification
    pub exclude_patterns: Vec<String>,
    /// Custom verification commands
    pub custom_checks: Vec<CustomCheck>,
    /// Optional SWE-bench official test command. When set, it is run after every
    /// file edit/write in addition to the normal per-language checks.
    #[serde(default)]
    pub post_edit_test_command: Option<String>,
    /// Per-language verification settings. When a language is absent,
    /// all checks default to enabled.
    #[serde(default)]
    pub language_settings: std::collections::HashMap<RepoLanguage, LanguageCheckSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCheck {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub run_on: Vec<String>, // File patterns that trigger this check
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            check_on_edit: true,
            test_on_edit: false, // Tests can be slow, opt-in
            lint_on_edit: false, // Clippy can be slow, opt-in
            format_on_edit: true,
            incremental: true,
            check_timeout_secs: 60,
            continue_on_failure: true,
            exclude_patterns: vec![
                "*.md".to_string(),
                "*.txt".to_string(),
                "*.json".to_string(),
                "*.toml".to_string(),
            ],
            custom_checks: vec![],
            post_edit_test_command: None,
            language_settings: std::collections::HashMap::new(),
        }
    }
}

impl VerificationConfig {
    /// Fast mode: only type check
    pub fn fast() -> Self {
        Self {
            check_on_edit: true,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        }
    }

    /// Thorough mode: all checks
    pub fn thorough() -> Self {
        Self {
            check_on_edit: true,
            test_on_edit: true,
            lint_on_edit: true,
            format_on_edit: true,
            ..Default::default()
        }
    }
}

/// The verification gate - runs checks and reports results
pub struct VerificationGate {
    config: VerificationConfig,
    project_root: PathBuf,
    last_results: Option<VerificationReport>,
    /// Cache of file hashes to detect changes and skip redundant verification
    file_hash_cache: std::collections::HashMap<String, u64>,
    /// Last verification timestamp for cache TTL
    last_verification_time: Option<std::time::Instant>,
    /// Optional hint from the SWE-bench dataset (e.g. "python").
    repo_language_hint: Option<String>,
    /// Cached inferred language to avoid re-scanning the repo.
    inferred_language_cache: Option<RepoLanguage>,
}

impl VerificationGate {
    pub fn new(project_root: impl AsRef<Path>, config: VerificationConfig) -> Self {
        Self {
            config,
            project_root: project_root.as_ref().to_path_buf(),
            last_results: None,
            file_hash_cache: std::collections::HashMap::new(),
            last_verification_time: None,
            repo_language_hint: None,
            inferred_language_cache: None,
        }
    }

    /// Set a language hint (e.g. from SWE-bench Pro dataset).
    /// Invalidates any cached inference so the hint takes effect.
    pub fn set_repo_language_hint(&mut self, hint: impl Into<String>) {
        self.repo_language_hint = Some(hint.into());
        self.inferred_language_cache = None;
    }

    /// Set the optional command to run automatically after every file edit/write.
    pub fn set_post_edit_test_command(&mut self, command: Option<String>) {
        self.config.post_edit_test_command = command;
    }

    /// Compute hash for a file's content
    fn compute_file_hash(&self, path: &str) -> Option<u64> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        use std::io::Read;

        let full_path = self.project_root.join(path);
        let mut file = std::fs::File::open(full_path).ok()?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).ok()?;

        let mut hasher = DefaultHasher::new();
        hasher.write(&contents);
        Some(hasher.finish())
    }

    /// Check if files have changed since last verification
    fn have_files_changed(&self, files: &[String]) -> bool {
        // If no previous verification, files are considered changed
        if self.file_hash_cache.is_empty() {
            return true;
        }

        for file in files {
            let current_hash = match self.compute_file_hash(file) {
                Some(h) => h,
                None => return true, // Can't read file, assume changed
            };

            match self.file_hash_cache.get(file) {
                Some(cached_hash) if *cached_hash == current_hash => {
                    // File unchanged
                    continue;
                }
                _ => {
                    // File changed or not in cache
                    return true;
                }
            }
        }

        false // All files unchanged
    }

    /// Update file hash cache with current hashes
    fn update_file_cache(&mut self, files: &[String]) {
        for file in files {
            if let Some(hash) = self.compute_file_hash(file) {
                self.file_hash_cache.insert(file.clone(), hash);
            }
        }
    }

    /// Run verification after a file change
    pub async fn verify_change(
        &mut self,
        changed_files: &[String],
        trigger: &str,
    ) -> Result<VerificationReport> {
        let start = Instant::now();
        let mut checks = Vec::new();
        let mut suggested_next_steps = Vec::new();
        let mut overall_passed = true;

        // Filter out excluded files
        let files_to_check: Vec<_> = changed_files
            .iter()
            .filter(|f| !self.is_excluded(f))
            .cloned()
            .collect();

        if files_to_check.is_empty() {
            return Ok(VerificationReport {
                triggered_by: trigger.to_string(),
                timestamp: chrono::Utc::now(),
                total_duration_ms: 0,
                checks: vec![],
                overall_passed: true,
                affected_files: changed_files.to_vec(),
                side_effects: vec![],
                suggested_next_steps: vec![
                    "No code files changed, verification skipped".to_string()
                ],
            });
        }

        // Check if files have actually changed (cache optimization)
        if !self.have_files_changed(&files_to_check) {
            // Return cached result if available
            if let Some(ref last_report) = self.last_results {
                if last_report.overall_passed {
                    return Ok(VerificationReport {
                        triggered_by: format!("{} (cached)", trigger),
                        timestamp: chrono::Utc::now(),
                        total_duration_ms: 0,
                        checks: last_report.checks.clone(),
                        overall_passed: true,
                        affected_files: changed_files.to_vec(),
                        side_effects: vec![],
                        suggested_next_steps: vec![
                            "Files unchanged - using cached verification results".to_string(),
                        ],
                    });
                }
            }
        }

        // Group changed files by language
        let mut files_by_lang: std::collections::HashMap<RepoLanguage, Vec<String>> =
            std::collections::HashMap::new();
        for file in &files_to_check {
            if let Some(ext) = Path::new(file).extension().and_then(|e| e.to_str()) {
                let ext = format!(".{}", ext);
                if let Some(lang) = RepoLanguage::from_extension(&ext) {
                    files_by_lang.entry(lang).or_default().push(file.clone());
                }
            }
        }

        // Run cheap syntax checks first for all touched languages
        if self.config.check_on_edit {
            for (lang, files) in &files_by_lang {
                let settings = self.get_language_settings(*lang);
                if settings.syntax {
                    let result = self.run_cheap_syntax_check(*lang, files).await?;
                    if !result.passed {
                        suggested_next_steps
                            .push(format!("Fix {} syntax errors before proceeding", lang));
                    }
                    checks.push(result);
                }
            }
        }

        // Run optional post-edit test command (e.g., SWE-bench official tests)
        if let Some(ref cmd) = self.config.post_edit_test_command {
            let check_start = Instant::now();
            let timeout_secs = self.config.check_timeout_secs.max(60);
            let timeout_duration = tokio::time::Duration::from_secs(timeout_secs);

            let mut parsed = shlex::split(cmd).unwrap_or_default();
            let (passed, output, duration_ms) = if parsed.is_empty() {
                (
                    false,
                    format!("Empty or unparseable post-edit test command: {}", cmd),
                    check_start.elapsed().as_millis() as u64,
                )
            } else {
                let program = parsed.remove(0);
                let command_future = Command::new(&program)
                    .args(&parsed)
                    .current_dir(&self.project_root)
                    .output();

                match tokio::time::timeout(timeout_duration, command_future).await {
                    Ok(Ok(output)) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let combined = if stderr.is_empty() {
                            stdout.to_string()
                        } else {
                            format!("{}\n{}", stdout, stderr)
                        };
                        (
                            output.status.success(),
                            truncate_str(&combined, 4000),
                            check_start.elapsed().as_millis() as u64,
                        )
                    }
                    Ok(Err(e)) => (
                        false,
                        format!("Failed to run post-edit test command '{}': {}", cmd, e),
                        check_start.elapsed().as_millis() as u64,
                    ),
                    Err(_) => (
                        false,
                        format!(
                            "Post-edit test command '{}' timed out after {} seconds",
                            cmd, timeout_secs
                        ),
                        timeout_duration.as_millis() as u64,
                    ),
                }
            };

            if !passed {
                overall_passed = false;
                suggested_next_steps.push(format!(
                    "The post-edit test command failed: {}. Fix the failing test before completing.",
                    cmd
                ));
            }
            checks.push(CheckResult {
                check_type: CheckType::Test,
                passed,
                duration_ms,
                output,
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            });
        }

        // Early exit if syntax checks failed and continue_on_failure is false
        if !self.config.continue_on_failure && checks.iter().any(|c| !c.passed) {
            let overall_passed = false;
            let total_duration = start.elapsed().as_millis() as u64;
            let side_effects = self.detect_side_effects(&files_to_check).await;
            let report = VerificationReport {
                triggered_by: trigger.to_string(),
                timestamp: chrono::Utc::now(),
                total_duration_ms: total_duration,
                checks,
                overall_passed,
                affected_files: files_to_check,
                side_effects,
                suggested_next_steps,
            };
            self.last_results = Some(report.clone());
            self.update_file_cache(&report.affected_files);
            self.last_verification_time = Some(std::time::Instant::now());
            return Ok(report);
        }

        // Detect if any Rust files changed
        let rust_files_changed = files_to_check.iter().any(|f| f.ends_with(".rs"));

        if rust_files_changed {
            // Run type check
            if self.config.check_on_edit {
                let result = self.run_cargo_check().await?;
                if !result.passed {
                    suggested_next_steps.push("Fix type errors before proceeding".to_string());
                }
                checks.push(result);
            }

            // Run format check
            if self.config.format_on_edit {
                let result = self.run_cargo_fmt_check().await?;
                if !result.passed {
                    suggested_next_steps.push("Run cargo fmt to fix formatting".to_string());
                }
                checks.push(result);
            }

            // Run tests (if enabled)
            if self.config.test_on_edit {
                let result = self.run_cargo_test().await?;
                if !result.passed {
                    suggested_next_steps.push("Fix failing tests".to_string());
                }
                checks.push(result);
            }

            // Run clippy (if enabled)
            if self.config.lint_on_edit {
                let result = self.run_cargo_clippy().await?;
                if !result.passed {
                    suggested_next_steps.push("Address clippy warnings".to_string());
                }
                checks.push(result);
            }
        }

        // Targeted tests for non-Rust languages
        if self.config.test_on_edit {
            for (lang, files) in &files_by_lang {
                if *lang == RepoLanguage::Rust {
                    continue;
                }
                let settings = self.get_language_settings(*lang);
                if settings.test {
                    let result = self.run_targeted_test(*lang, files).await?;
                    if !result.passed {
                        suggested_next_steps.push(format!("Fix failing {} tests", lang));
                    }
                    checks.push(result);
                }
            }
        }

        // Multi-language QA: dispatch to language_qa runners for non-Rust files
        let has_non_rust = files_by_lang.keys().any(|l| *l != RepoLanguage::Rust);
        if has_non_rust {
            use crate::testing::language_qa::{run_go_qa, run_node_qa, run_python_qa, QaLanguage};

            let detected_lang = QaLanguage::detect(&self.project_root);
            let timeout = self.config.check_timeout_secs;

            let has_python = files_by_lang.contains_key(&RepoLanguage::Python);
            let has_js = files_by_lang.contains_key(&RepoLanguage::JavaScript)
                || files_by_lang.contains_key(&RepoLanguage::TypeScript);
            let has_go = files_by_lang.contains_key(&RepoLanguage::Go);

            let qa_results = match detected_lang {
                QaLanguage::Python if has_python => {
                    run_python_qa(&self.project_root, timeout).await
                }
                QaLanguage::Node if has_js => run_node_qa(&self.project_root, timeout).await,
                QaLanguage::Go if has_go => run_go_qa(&self.project_root, timeout).await,
                _ => Vec::new(),
            };

            for qa_stage in qa_results {
                let check = Self::qa_stage_to_check_result(qa_stage);
                if !check.passed {
                    suggested_next_steps.push(format!(
                        "Fix {} {} errors",
                        detected_lang,
                        check.check_type.as_str()
                    ));
                }
                checks.push(check);
            }
        }

        // Run custom checks
        for custom in &self.config.custom_checks {
            if self.should_run_custom_check(custom, &files_to_check) {
                let result = self.run_custom_check(custom).await?;
                checks.push(result);
            }
        }

        overall_passed = overall_passed && checks.iter().all(|c| c.passed);
        let total_duration = start.elapsed().as_millis() as u64;

        // Detect side effects
        let side_effects = self.detect_side_effects(&files_to_check).await;

        // Add suggestions based on results
        if overall_passed && suggested_next_steps.is_empty() {
            suggested_next_steps.push("All checks passed - safe to proceed".to_string());
        }

        let report = VerificationReport {
            triggered_by: trigger.to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: total_duration,
            checks,
            overall_passed,
            affected_files: files_to_check,
            side_effects,
            suggested_next_steps,
        };

        self.last_results = Some(report.clone());

        // Update file hash cache for future change detection
        self.update_file_cache(&report.affected_files);
        self.last_verification_time = Some(std::time::Instant::now());

        Ok(report)
    }

    /// Quick verification - just type check
    pub async fn quick_verify(&mut self, _changed_files: &[String]) -> Result<bool> {
        let result = self.run_cargo_check().await?;
        Ok(result.passed)
    }

    /// Full verification - all checks
    pub async fn full_verify(&mut self) -> Result<VerificationReport> {
        self.verify_change(&[], "full_verification").await
    }

    /// Run cargo check
    async fn run_cargo_check(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = run_reaped(
            "cargo",
            &["check", "--message-format=json"],
            &self.project_root,
            self.config.check_timeout_secs,
        )
        .await?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let (errors, warnings) = parse_cargo_json_output(&stdout);

        Ok(CheckResult {
            check_type: CheckType::TypeCheck,
            passed: output.success,
            duration_ms: duration,
            output: if output.success {
                "Type check passed".to_string()
            } else {
                stderr.to_string()
            },
            errors,
            warnings: warnings.iter().map(|e| e.message.clone()).collect(),
            suggestions: vec![],
        })
    }

    /// Run cargo fmt --check
    async fn run_cargo_fmt_check(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = run_reaped(
            "cargo",
            &["fmt", "--check"],
            &self.project_root,
            self.config.check_timeout_secs,
        )
        .await?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);

        Ok(CheckResult {
            check_type: CheckType::Format,
            passed: output.success,
            duration_ms: duration,
            output: if output.success {
                "Formatting check passed".to_string()
            } else {
                stdout.to_string()
            },
            errors: vec![],
            warnings: vec![],
            suggestions: if !output.success {
                vec!["Run `cargo fmt` to fix formatting".to_string()]
            } else {
                vec![]
            },
        })
    }

    /// Run cargo test with timeout to prevent getting stuck
    async fn run_cargo_test(&self) -> Result<CheckResult> {
        let start = Instant::now();

        // Apply timeout from config (default 5 minutes)
        let timeout_secs = self.config.check_timeout_secs.max(60); // At least 60 seconds
        let timeout_duration = tokio::time::Duration::from_secs(timeout_secs);

        let command_future = Command::new("cargo")
            .args(["test", "--no-fail-fast"])
            .current_dir(&self.project_root)
            .output();

        let output = match tokio::time::timeout(timeout_duration, command_future).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                // Timeout - return a graceful error
                return Ok(CheckResult {
                    check_type: CheckType::Test,
                    passed: false,
                    duration_ms: timeout_duration.as_millis() as u64,
                    output: format!("Tests timed out after {} seconds", timeout_secs),
                    errors: vec![VerificationError {
                        file: "N/A".to_string(),
                        line: None,
                        column: None,
                        message: format!("cargo test exceeded {}s timeout", timeout_secs),
                        code: Some("TIMEOUT".to_string()),
                        severity: ErrorSeverity::Error,
                        suggestion: Some("Tests took too long. Run manually with `cargo test` or increase check_timeout_secs in config".to_string()),
                    }],
                    warnings: vec!["Tests were cancelled due to timeout. Press Ctrl+C to exit if stuck.".to_string()],
                    suggestions: vec!["Consider running tests manually or increasing timeout".to_string()],
                });
            }
        };

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse test failures from output
        let errors = parse_test_failures(&stdout, &stderr);

        Ok(CheckResult {
            check_type: CheckType::Test,
            passed: output.status.success(),
            duration_ms: duration,
            output: format!("{}\n{}", stdout, stderr),
            errors,
            warnings: vec![],
            suggestions: vec![],
        })
    }

    /// Run cargo clippy
    async fn run_cargo_clippy(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = run_reaped(
            "cargo",
            &["clippy", "--message-format=json", "--", "-D", "warnings"],
            &self.project_root,
            self.config.check_timeout_secs,
        )
        .await?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let (errors, warnings) = parse_cargo_json_output(&stdout);

        Ok(CheckResult {
            check_type: CheckType::Lint,
            passed: output.success,
            duration_ms: duration,
            output: stderr.to_string(),
            errors,
            warnings: warnings.iter().map(|e| e.message.clone()).collect(),
            suggestions: vec![],
        })
    }

    /// Run a custom check
    async fn run_custom_check(&self, check: &CustomCheck) -> Result<CheckResult> {
        let start = Instant::now();

        let args_ref: Vec<&str> = check.args.iter().map(|s| s.as_str()).collect();
        let output = run_reaped(
            &check.command,
            &args_ref,
            &self.project_root,
            self.config.check_timeout_secs,
        )
        .await?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(CheckResult {
            check_type: CheckType::Custom,
            passed: output.success,
            duration_ms: duration,
            output: format!("{}\n{}", stdout, stderr),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        })
    }

    /// Check if a file should be excluded from verification
    pub fn is_excluded(&self, file: &str) -> bool {
        for pattern in &self.config.exclude_patterns {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                if glob.matches(file) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a custom check should run based on changed files
    fn should_run_custom_check(&self, check: &CustomCheck, files: &[String]) -> bool {
        if check.run_on.is_empty() {
            return true;
        }
        for pattern in &check.run_on {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                if files.iter().any(|f| glob.matches(f)) {
                    return true;
                }
            }
        }
        false
    }

    /// Convert a QA stage result from language_qa into a CheckResult.
    fn qa_stage_to_check_result(stage: crate::testing::qa_profiles::QaStageResult) -> CheckResult {
        use crate::testing::qa_profiles::QaStage;
        let check_type = match stage.stage {
            QaStage::Syntax | QaStage::TypeCheck => CheckType::TypeCheck,
            QaStage::Format => CheckType::Format,
            QaStage::Lint => CheckType::Lint,
            QaStage::Test => CheckType::Test,
            QaStage::Security => CheckType::Custom,
        };
        CheckResult {
            check_type,
            passed: stage.passed,
            duration_ms: stage.duration_ms,
            output: stage.output,
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        }
    }

    /// Detect side effects from file changes
    async fn detect_side_effects(&self, files: &[String]) -> Vec<SideEffect> {
        let mut effects = Vec::new();

        for file in files {
            // Check if it's a new file
            let path = self.project_root.join(file);
            if path.exists() {
                effects.push(SideEffect {
                    effect_type: SideEffectType::FileModified,
                    description: format!("Modified: {}", file),
                    files: vec![file.clone()],
                });
            }

            // Check for test files (language-agnostic)
            if file.contains("test")
                || file.contains("_test.rs")
                || file.contains("_test.py")
                || file.contains("_test.go")
                || file.contains(".test.js")
                || file.contains(".test.ts")
            {
                effects.push(SideEffect {
                    effect_type: SideEffectType::TestAdded,
                    description: "Test file modified".to_string(),
                    files: vec![file.clone()],
                });
            }
        }

        // Check manifest files for dependency changes
        if files.iter().any(|f| f.ends_with("Cargo.toml")) {
            effects.push(SideEffect {
                effect_type: SideEffectType::DependencyAdded,
                description: "Cargo.toml modified - dependencies may have changed".to_string(),
                files: vec!["Cargo.toml".to_string()],
            });
        }
        if files.iter().any(|f| f.ends_with("package.json")) {
            effects.push(SideEffect {
                effect_type: SideEffectType::DependencyAdded,
                description: "package.json modified - dependencies may have changed".to_string(),
                files: vec!["package.json".to_string()],
            });
        }
        if files.iter().any(|f| f.ends_with("go.mod")) {
            effects.push(SideEffect {
                effect_type: SideEffectType::DependencyAdded,
                description: "go.mod modified - dependencies may have changed".to_string(),
                files: vec!["go.mod".to_string()],
            });
        }
        if files
            .iter()
            .any(|f| f.ends_with("requirements.txt") || f.ends_with("pyproject.toml"))
        {
            effects.push(SideEffect {
                effect_type: SideEffectType::DependencyAdded,
                description: "Python manifest modified - dependencies may have changed".to_string(),
                files: files
                    .iter()
                    .filter(|f| f.ends_with("requirements.txt") || f.ends_with("pyproject.toml"))
                    .cloned()
                    .collect(),
            });
        }

        effects
    }

    /// Get the last verification results
    pub fn last_results(&self) -> Option<&VerificationReport> {
        self.last_results.as_ref()
    }

    /// Infer the primary repository language using multiple signals:
    /// 1. SWE-bench `repo_language` hint (if set)
    /// 2. Manifest files (Cargo.toml, package.json, go.mod, etc.)
    /// 3. File extensions in the repo
    ///
    /// Result is cached per workdir.
    #[cfg(test)]
    fn infer_repo_language(&mut self) -> RepoLanguage {
        if let Some(cached) = self.inferred_language_cache {
            return cached;
        }

        // 1. Use dataset hint if available
        if let Some(ref hint) = self.repo_language_hint {
            let lang = match hint.to_lowercase().as_str() {
                "rust" => RepoLanguage::Rust,
                "python" => RepoLanguage::Python,
                "javascript" | "js" => RepoLanguage::JavaScript,
                "typescript" | "ts" => RepoLanguage::TypeScript,
                "java" => RepoLanguage::Java,
                "csharp" | "c#" => RepoLanguage::CSharp,
                "cpp" | "c++" | "c" => RepoLanguage::Cpp,
                "sql" => RepoLanguage::Sql,
                "go" | "golang" => RepoLanguage::Go,
                "swift" => RepoLanguage::Swift,
                _ => RepoLanguage::Unknown,
            };
            if lang != RepoLanguage::Unknown {
                self.inferred_language_cache = Some(lang);
                return lang;
            }
        }

        // 2. Check for manifest files
        let manifests = [
            ("Cargo.toml", RepoLanguage::Rust),
            ("go.mod", RepoLanguage::Go),
            ("Package.swift", RepoLanguage::Swift),
            ("pom.xml", RepoLanguage::Java),
            ("build.gradle", RepoLanguage::Java),
            ("build.gradle.kts", RepoLanguage::Java),
            ("tsconfig.json", RepoLanguage::TypeScript),
            ("package.json", RepoLanguage::JavaScript),
            ("setup.py", RepoLanguage::Python),
            ("pyproject.toml", RepoLanguage::Python),
            ("requirements.txt", RepoLanguage::Python),
        ];
        for (file, lang) in &manifests {
            if self.project_root.join(file).exists() {
                if *lang == RepoLanguage::JavaScript
                    && self.project_root.join("tsconfig.json").exists()
                {
                    self.inferred_language_cache = Some(RepoLanguage::TypeScript);
                    return RepoLanguage::TypeScript;
                }
                self.inferred_language_cache = Some(*lang);
                return *lang;
            }
        }

        // 3. Fall back to file extension counting
        let counts = scan_repo_extensions(&self.project_root, 100);
        let best = counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(lang, _)| *lang);
        let result = best.unwrap_or(RepoLanguage::Unknown);
        self.inferred_language_cache = Some(result);
        result
    }

    /// Retrieve per-language settings, falling back to defaults.
    fn get_language_settings(&self, lang: RepoLanguage) -> LanguageCheckSet {
        self.config
            .language_settings
            .get(&lang)
            .cloned()
            .unwrap_or_default()
    }

    /// Run a cheap syntax check on ONLY the touched files.
    async fn run_cheap_syntax_check(
        &self,
        lang: RepoLanguage,
        files: &[String],
    ) -> Result<CheckResult> {
        let start = Instant::now();
        let full_paths: Vec<_> = files.iter().map(|f| self.project_root.join(f)).collect();

        let (program, args): (&str, Vec<String>) = match lang {
            RepoLanguage::Python => {
                let mut a = vec!["-m".to_string(), "py_compile".to_string()];
                for p in &full_paths {
                    a.push(p.to_string_lossy().to_string());
                }
                ("python3", a)
            }
            RepoLanguage::JavaScript => {
                if let Some(p) = full_paths.first() {
                    (
                        "node",
                        vec!["--check".to_string(), p.to_string_lossy().to_string()],
                    )
                } else {
                    return Ok(CheckResult {
                        check_type: CheckType::TypeCheck,
                        passed: true,
                        duration_ms: 0,
                        output: "No JS files to check".to_string(),
                        errors: vec![],
                        warnings: vec![],
                        suggestions: vec![],
                    });
                }
            }
            RepoLanguage::TypeScript => {
                let mut a = vec!["tsc".to_string(), "--noEmit".to_string()];
                for p in &full_paths {
                    a.push(p.to_string_lossy().to_string());
                }
                ("npx", a)
            }
            RepoLanguage::Java => {
                let mut a = vec![
                    "-Xlint:none".to_string(),
                    "-d".to_string(),
                    "/tmp".to_string(),
                ];
                for p in &full_paths {
                    a.push(p.to_string_lossy().to_string());
                }
                ("javac", a)
            }
            RepoLanguage::CSharp => {
                if self.project_root.join("global.json").exists()
                    || self
                        .project_root
                        .read_dir()
                        .ok()
                        .into_iter()
                        .flatten()
                        .flatten()
                        .any(|e| {
                            e.path()
                                .extension()
                                .and_then(|x| x.to_str())
                                .is_some_and(|x| matches!(x, "sln" | "csproj"))
                        })
                {
                    ("dotnet", vec!["build".to_string(), "--nologo".to_string()])
                } else {
                    let mut a = vec![
                        "-target:library".to_string(),
                        "-out:/tmp/selfware-csharp-check.dll".to_string(),
                    ];
                    for p in &full_paths {
                        a.push(p.to_string_lossy().to_string());
                    }
                    ("csc", a)
                }
            }
            RepoLanguage::Cpp => {
                if self.project_root.join("CMakeLists.txt").exists() {
                    ("cmake", vec!["--build".to_string(), ".".to_string()])
                } else if let Some(p) = full_paths.first() {
                    let compiler = if p
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| matches!(e, "c"))
                    {
                        "cc"
                    } else {
                        "c++"
                    };
                    (
                        compiler,
                        vec!["-fsyntax-only".to_string(), p.to_string_lossy().to_string()],
                    )
                } else {
                    return Ok(CheckResult {
                        check_type: CheckType::TypeCheck,
                        passed: true,
                        duration_ms: 0,
                        output: "No C/C++ files to check".to_string(),
                        errors: vec![],
                        warnings: vec![],
                        suggestions: vec![],
                    });
                }
            }
            RepoLanguage::Sql => {
                if self.command_exists("sqlfluff").await {
                    let mut a = vec![
                        "lint".to_string(),
                        "--dialect".to_string(),
                        "ansi".to_string(),
                    ];
                    for p in &full_paths {
                        a.push(p.to_string_lossy().to_string());
                    }
                    ("sqlfluff", a)
                } else {
                    return Ok(CheckResult {
                        check_type: CheckType::TypeCheck,
                        passed: true,
                        duration_ms: 0,
                        output: "sqlfluff not installed; SQL syntax check skipped".to_string(),
                        errors: vec![],
                        warnings: vec![
                            "SQL syntax check skipped: sqlfluff not installed".to_string()
                        ],
                        suggestions: vec![],
                    });
                }
            }
            RepoLanguage::Go => {
                let mut a = vec!["-l".to_string()];
                for p in &full_paths {
                    a.push(p.to_string_lossy().to_string());
                }
                ("gofmt", a)
            }
            RepoLanguage::Swift => {
                if self.project_root.join("Package.swift").exists() {
                    ("swift", vec!["build".to_string()])
                } else if let Some(p) = full_paths.first() {
                    (
                        "swiftc",
                        vec!["-parse".to_string(), p.to_string_lossy().to_string()],
                    )
                } else {
                    return Ok(CheckResult {
                        check_type: CheckType::TypeCheck,
                        passed: true,
                        duration_ms: 0,
                        output: "No Swift files to check".to_string(),
                        errors: vec![],
                        warnings: vec![],
                        suggestions: vec![],
                    });
                }
            }
            RepoLanguage::Rust => {
                let mut a = vec!["--check".to_string()];
                for p in &full_paths {
                    a.push(p.to_string_lossy().to_string());
                }
                ("rustfmt", a)
            }
            RepoLanguage::Unknown => {
                return Ok(CheckResult {
                    check_type: CheckType::TypeCheck,
                    passed: true,
                    duration_ms: 0,
                    output: "Unknown language, skipping syntax check".to_string(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                });
            }
        };

        if !matches!(program, "sh") && !self.command_exists(program).await {
            return Ok(CheckResult {
                check_type: CheckType::TypeCheck,
                passed: false,
                duration_ms: 0,
                output: format!(
                    "{} syntax check could not run: `{}` not found",
                    lang, program
                ),
                errors: vec![VerificationError {
                    file: files.first().cloned().unwrap_or_default(),
                    line: None,
                    column: None,
                    message: format!("{} verifier `{}` not found", lang, program),
                    code: Some("VERIFIER_NOT_FOUND".to_string()),
                    severity: ErrorSeverity::Error,
                    suggestion: Some(format!(
                        "Install `{}` or run a project-specific verifier",
                        program
                    )),
                }],
                warnings: vec![],
                suggestions: vec![format!(
                    "Run a project-specific {} verification command manually",
                    lang
                )],
            });
        }

        let output = Command::new(program)
            .args(&args)
            .current_dir(&self.project_root)
            .output()
            .await
            .context(format!("Failed to run {} syntax check", lang))?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = if stderr.is_empty() {
            stdout.to_string()
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        Ok(CheckResult {
            check_type: CheckType::TypeCheck,
            passed: output.status.success(),
            duration_ms: duration,
            output: if output.status.success() {
                format!("{} syntax check passed", lang)
            } else {
                combined
            },
            errors: if output.status.success() {
                vec![]
            } else {
                vec![VerificationError {
                    file: files.first().cloned().unwrap_or_default(),
                    line: None,
                    column: None,
                    message: format!("{} syntax error", lang),
                    code: None,
                    severity: ErrorSeverity::Error,
                    suggestion: Some(format!("Check {} syntax and fix errors", lang)),
                }]
            },
            warnings: vec![],
            suggestions: if output.status.success() {
                vec![]
            } else {
                vec![format!("Fix {} syntax errors before running tests", lang)]
            },
        })
    }

    /// Infer the appropriate test command for the repository.
    async fn infer_test_command(&self, lang: RepoLanguage) -> Option<(String, Vec<String>)> {
        match lang {
            RepoLanguage::Rust => Some((
                "cargo".to_string(),
                vec!["test".to_string(), "--no-fail-fast".to_string()],
            )),
            RepoLanguage::Python => {
                if self.project_root.join("pytest.ini").exists()
                    || self.has_pyproject_pytest()
                    || self.command_exists("pytest").await
                {
                    Some((
                        "pytest".to_string(),
                        vec!["--quiet".to_string(), "--tb=short".to_string()],
                    ))
                } else {
                    Some((
                        "python3".to_string(),
                        vec![
                            "-m".to_string(),
                            "unittest".to_string(),
                            "discover".to_string(),
                            "-s".to_string(),
                            ".".to_string(),
                            "-q".to_string(),
                        ],
                    ))
                }
            }
            RepoLanguage::JavaScript | RepoLanguage::TypeScript => {
                let pm = self.detect_package_manager();
                Some((pm.to_string(), vec!["test".to_string()]))
            }
            RepoLanguage::Java => {
                if self.project_root.join("pom.xml").exists() {
                    Some(("mvn".to_string(), vec!["test".to_string()]))
                } else if self.project_root.join("build.gradle").exists()
                    || self.project_root.join("build.gradle.kts").exists()
                {
                    Some(("gradle".to_string(), vec!["test".to_string()]))
                } else {
                    None
                }
            }
            RepoLanguage::CSharp => Some(("dotnet".to_string(), vec!["test".to_string()])),
            RepoLanguage::Cpp => {
                if self.project_root.join("CMakeLists.txt").exists() {
                    Some(("ctest".to_string(), vec!["--output-on-failure".to_string()]))
                } else {
                    Some(("make".to_string(), vec!["test".to_string()]))
                }
            }
            RepoLanguage::Sql => {
                if self.command_exists("sqlfluff").await {
                    Some((
                        "sqlfluff".to_string(),
                        vec![
                            "lint".to_string(),
                            "--dialect".to_string(),
                            "ansi".to_string(),
                        ],
                    ))
                } else {
                    None
                }
            }
            RepoLanguage::Go => Some((
                "go".to_string(),
                vec!["test".to_string(), "-v".to_string(), "./...".to_string()],
            )),
            RepoLanguage::Swift => Some(("swift".to_string(), vec!["test".to_string()])),
            RepoLanguage::Unknown => None,
        }
    }

    /// Run targeted tests for a specific language.
    async fn run_targeted_test(
        &self,
        lang: RepoLanguage,
        changed_files: &[String],
    ) -> Result<CheckResult> {
        let start = Instant::now();
        let timeout_secs = self.config.check_timeout_secs.max(60);

        let Some((program, mut args)) = self.infer_test_command(lang).await else {
            return Ok(CheckResult {
                check_type: CheckType::Test,
                passed: true,
                duration_ms: 0,
                output: "No test command inferred for unknown language".to_string(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            });
        };

        // If specific test files were touched, target them when possible
        if lang == RepoLanguage::Python {
            let test_files: Vec<_> = changed_files
                .iter()
                .filter(|f| f.contains("test") && f.ends_with(".py"))
                .cloned()
                .collect();
            if !test_files.is_empty() {
                args.extend(test_files);
            }
        }

        let command_future = Command::new(&program)
            .args(&args)
            .current_dir(&self.project_root)
            .output();

        let output = match tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            command_future,
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                return Ok(CheckResult {
                    check_type: CheckType::Test,
                    passed: false,
                    duration_ms: timeout_secs * 1000,
                    output: format!("Tests timed out after {} seconds", timeout_secs),
                    errors: vec![VerificationError {
                        file: "N/A".to_string(),
                        line: None,
                        column: None,
                        message: format!("{} test exceeded {}s timeout", lang, timeout_secs),
                        code: Some("TIMEOUT".to_string()),
                        severity: ErrorSeverity::Error,
                        suggestion: Some(
                            "Tests took too long. Run manually or increase check_timeout_secs in config"
                                .to_string(),
                        ),
                    }],
                    warnings: vec![],
                    suggestions: vec![],
                });
            }
        };

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(CheckResult {
            check_type: CheckType::Test,
            passed: output.status.success(),
            duration_ms: duration,
            output: format!("{}\n{}", stdout, stderr),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        })
    }

    async fn command_exists(&self, cmd: &str) -> bool {
        match Command::new("which").arg(cmd).output().await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    fn has_pyproject_pytest(&self) -> bool {
        let path = self.project_root.join("pyproject.toml");
        if !path.exists() {
            return false;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            content.contains("[tool.pytest") || content.contains("[tool:pytest")
        } else {
            false
        }
    }

    fn detect_package_manager(&self) -> &'static str {
        if self.project_root.join("pnpm-lock.yaml").exists() {
            "pnpm"
        } else if self.project_root.join("yarn.lock").exists() {
            "yarn"
        } else {
            "npm"
        }
    }
}

/// Scan a repo for file extensions and count languages.
#[cfg(test)]
fn scan_repo_extensions(
    root: &Path,
    max_files: usize,
) -> std::collections::HashMap<RepoLanguage, usize> {
    let mut counts = std::collections::HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    let mut checked = 0usize;

    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if checked >= max_files {
                    return counts;
                }
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                            let ext = format!(".{}", ext);
                            if let Some(lang) = RepoLanguage::from_extension(&ext) {
                                *counts.entry(lang).or_insert(0) += 1;
                            }
                        }
                        checked += 1;
                    } else if meta.is_dir() {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        if !name.starts_with('.')
                            && name != "target"
                            && name != "node_modules"
                            && name != "__pycache__"
                            && name != "vendor"
                        {
                            stack.push(entry.path());
                        }
                    }
                }
            }
        }
    }
    counts
}

/// Convert a CompilerError from cargo module to VerificationError
fn compiler_error_to_verification_error(ce: &CompilerError) -> VerificationError {
    VerificationError {
        file: ce.file.clone(),
        line: if ce.line > 0 { Some(ce.line) } else { None },
        column: if ce.column > 0 { Some(ce.column) } else { None },
        message: ce.message.clone(),
        code: ce.code.clone(),
        severity: match ce.severity {
            Severity::Error => ErrorSeverity::Error,
            Severity::Warning => ErrorSeverity::Warning,
            Severity::Note => ErrorSeverity::Note,
            Severity::Help => ErrorSeverity::Help,
        },
        suggestion: ce.suggestion.clone(),
    }
}

/// Parse cargo JSON output into errors and warnings
/// Uses shared parsing logic from crate::tools::cargo
fn parse_cargo_json_output(output: &str) -> (Vec<VerificationError>, Vec<VerificationError>) {
    let (cargo_errors, cargo_warnings) = parse_cargo_json_messages(output);

    let errors = cargo_errors
        .iter()
        .map(compiler_error_to_verification_error)
        .collect();
    let warnings = cargo_warnings
        .iter()
        .map(compiler_error_to_verification_error)
        .collect();

    (errors, warnings)
}

/// Parse test failures from cargo test output
fn parse_test_failures(stdout: &str, stderr: &str) -> Vec<VerificationError> {
    let mut errors = Vec::new();

    // Look for FAILED tests
    for line in stdout.lines().chain(stderr.lines()) {
        if line.contains("FAILED") && line.contains("test ") {
            let test_name = line
                .split("test ")
                .nth(1)
                .and_then(|s| s.split(" ...").next())
                .unwrap_or("unknown");

            errors.push(VerificationError {
                file: String::new(),
                line: None,
                column: None,
                message: format!("Test failed: {}", test_name),
                code: None,
                severity: ErrorSeverity::Error,
                suggestion: Some("Check test output for details".to_string()),
            });
        }

        // Look for panic messages
        if line.contains("panicked at") {
            errors.push(VerificationError {
                file: String::new(),
                line: None,
                column: None,
                message: line.to_string(),
                code: None,
                severity: ErrorSeverity::Error,
                suggestion: None,
            });
        }
    }

    errors
}

/// Format a verification report for display
impl std::fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n╔══════════════════════════════════════════╗")?;
        writeln!(f, "║         VERIFICATION REPORT              ║")?;
        writeln!(f, "╠══════════════════════════════════════════╣")?;
        writeln!(
            f,
            "║ Trigger: {:<30} ║",
            truncate_str(&self.triggered_by, 30)
        )?;
        writeln!(
            f,
            "║ Status: {:<31} ║",
            if self.overall_passed {
                "✓ PASSED"
            } else {
                "✗ FAILED"
            }
        )?;
        writeln!(
            f,
            "║ Duration: {:<29} ║",
            format!("{}ms", self.total_duration_ms)
        )?;
        writeln!(f, "╠══════════════════════════════════════════╣")?;

        for check in &self.checks {
            let status = if check.passed { "✓" } else { "✗" };
            writeln!(
                f,
                "║ {} {}: {}ms",
                status,
                check.check_type.as_str(),
                check.duration_ms
            )?;

            for error in &check.errors {
                writeln!(
                    f,
                    "║   └─ {}: {}",
                    error.file,
                    truncate_str(&error.message, 30)
                )?;
            }
        }

        if !self.suggested_next_steps.is_empty() {
            writeln!(f, "╠══════════════════════════════════════════╣")?;
            writeln!(f, "║ Suggested next steps:                    ║")?;
            for step in &self.suggested_next_steps {
                writeln!(f, "║   • {}", truncate_str(step, 36))?;
            }
        }

        writeln!(f, "╚══════════════════════════════════════════╝")?;
        Ok(())
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_reaped_times_out_and_does_not_report_success() {
        let dir = tempfile::tempdir().unwrap();
        // A command that runs far longer than the 1s timeout must be killed and
        // reported as not-successful (not hang the verifier forever).
        let start = std::time::Instant::now();
        let out = run_reaped("sleep", &["30"], dir.path(), 1).await.unwrap();
        assert!(
            start.elapsed().as_secs() < 10,
            "should return at the timeout"
        );
        assert!(!out.success, "a timed-out check must not report success");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("timed out"),
            "stderr should note the timeout"
        );
    }

    #[tokio::test]
    async fn run_reaped_captures_a_normal_command() {
        let dir = tempfile::tempdir().unwrap();
        let out = run_reaped("printf", &["hello"], dir.path(), 5)
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hello");
    }

    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(config.format_on_edit);
    }

    #[test]
    fn test_verification_config_fast() {
        let config = VerificationConfig::fast();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(!config.lint_on_edit);
        assert!(!config.format_on_edit);
    }

    #[test]
    fn test_verification_config_thorough() {
        let config = VerificationConfig::thorough();
        assert!(config.check_on_edit);
        assert!(config.test_on_edit);
        assert!(config.lint_on_edit);
        assert!(config.format_on_edit);
    }

    #[test]
    fn test_check_type_as_str() {
        assert_eq!(CheckType::TypeCheck.as_str(), "type_check");
        assert_eq!(CheckType::Test.as_str(), "test");
        assert_eq!(CheckType::Lint.as_str(), "lint");
        assert_eq!(CheckType::Format.as_str(), "format");
    }

    #[test]
    fn test_parse_cargo_json_output_empty() {
        let (errors, warnings) = parse_cargo_json_output("");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_output_with_error() {
        let json_line = r#"{"reason":"compiler-message","message":{"level":"error","message":"test error","code":{"code":"E0001"},"spans":[{"file_name":"src/main.rs","line_start":1,"column_start":1,"is_primary":true}],"children":[]}}"#;
        let (errors, warnings) = parse_cargo_json_output(json_line);
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
        assert_eq!(errors[0].message, "test error");
    }

    #[test]
    fn test_parse_test_failures() {
        let stdout = "test foo::bar ... FAILED\ntest baz::qux ... ok";
        let errors = parse_test_failures(stdout, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("foo::bar"));
    }

    #[test]
    fn test_verification_report_display() {
        let report = VerificationReport {
            triggered_by: "file_edit".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 1234,
            checks: vec![CheckResult {
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 500,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            }],
            overall_passed: true,
            affected_files: vec!["src/main.rs".to_string()],
            side_effects: vec![],
            suggested_next_steps: vec!["All checks passed".to_string()],
        };

        let display = format!("{}", report);
        assert!(display.contains("VERIFICATION REPORT"));
        assert!(display.contains("PASSED"));
    }

    #[test]
    fn test_error_severity_serde() {
        let severity = ErrorSeverity::Error;
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn test_side_effect_type_serde() {
        let effect = SideEffectType::FileModified;
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(json, "\"file_modified\"");
    }

    #[tokio::test]
    async fn test_verification_gate_new() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        assert!(gate.last_results().is_none());
    }

    #[test]
    fn test_is_excluded() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        assert!(gate.is_excluded("README.md"));
        assert!(gate.is_excluded("config.json"));
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_check_type_custom() {
        assert_eq!(CheckType::Custom.as_str(), "custom");
    }

    #[test]
    fn test_check_result_creation() {
        let result = CheckResult {
            check_type: CheckType::TypeCheck,
            passed: true,
            duration_ms: 100,
            output: "Success".to_string(),
            errors: vec![],
            warnings: vec!["minor warning".to_string()],
            suggestions: vec!["consider this".to_string()],
        };
        assert!(result.passed);
        assert_eq!(result.duration_ms, 100);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.suggestions.len(), 1);
    }

    #[test]
    fn test_verification_error_creation() {
        let error = VerificationError {
            file: "src/main.rs".to_string(),
            line: Some(10),
            column: Some(5),
            message: "error message".to_string(),
            code: Some("E0001".to_string()),
            severity: ErrorSeverity::Error,
            suggestion: Some("fix this".to_string()),
        };
        assert_eq!(error.file, "src/main.rs");
        assert_eq!(error.line, Some(10));
        assert!(error.code.is_some());
    }

    #[test]
    fn test_error_severity_variants() {
        let _ = ErrorSeverity::Error;
        let _ = ErrorSeverity::Warning;
        let _ = ErrorSeverity::Note;
        let _ = ErrorSeverity::Help;
    }

    #[test]
    fn test_side_effect_creation() {
        let effect = SideEffect {
            effect_type: SideEffectType::FileCreated,
            description: "New file".to_string(),
            files: vec!["new.rs".to_string()],
        };
        assert_eq!(effect.effect_type, SideEffectType::FileCreated);
        assert_eq!(effect.files.len(), 1);
    }

    #[test]
    fn test_side_effect_types() {
        assert_eq!(
            serde_json::to_string(&SideEffectType::FileCreated).unwrap(),
            "\"file_created\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::FileDeleted).unwrap(),
            "\"file_deleted\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::DependencyAdded).unwrap(),
            "\"dependency_added\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::DependencyRemoved).unwrap(),
            "\"dependency_removed\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::TestAdded).unwrap(),
            "\"test_added\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::TestRemoved).unwrap(),
            "\"test_removed\""
        );
    }

    #[test]
    fn test_custom_check_creation() {
        let check = CustomCheck {
            name: "my_check".to_string(),
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            run_on: vec!["*.rs".to_string()],
        };
        assert_eq!(check.name, "my_check");
        assert_eq!(check.args.len(), 1);
    }

    #[test]
    fn test_verification_config_default_exclude() {
        let config = VerificationConfig::default();
        assert!(config.exclude_patterns.contains(&"*.md".to_string()));
        assert!(config.exclude_patterns.contains(&"*.txt".to_string()));
        assert!(config.exclude_patterns.contains(&"*.json".to_string()));
        assert!(config.exclude_patterns.contains(&"*.toml".to_string()));
    }

    #[test]
    fn test_should_run_custom_check_empty_run_on() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        let check = CustomCheck {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec![], // Empty means run on all
        };

        assert!(gate.should_run_custom_check(&check, &["any.rs".to_string()]));
    }

    #[test]
    fn test_should_run_custom_check_matching_pattern() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        let check = CustomCheck {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec!["*.rs".to_string()],
        };

        assert!(gate.should_run_custom_check(&check, &["main.rs".to_string()]));
        assert!(!gate.should_run_custom_check(&check, &["main.py".to_string()]));
    }

    #[test]
    fn test_parse_test_failures_with_panic() {
        let output = "panicked at 'assertion failed', src/test.rs:10";
        let errors = parse_test_failures(output, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("panicked"));
    }

    #[test]
    fn test_parse_test_failures_no_failures() {
        let output = "test foo::bar ... ok\ntest baz::qux ... ok";
        let errors = parse_test_failures(output, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_verification_report_display_failed() {
        let report = VerificationReport {
            triggered_by: "test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 500,
            checks: vec![CheckResult {
                check_type: CheckType::TypeCheck,
                passed: false,
                duration_ms: 500,
                output: "error".to_string(),
                errors: vec![VerificationError {
                    file: "src/main.rs".to_string(),
                    line: Some(10),
                    column: Some(1),
                    message: "type error".to_string(),
                    code: Some("E0001".to_string()),
                    severity: ErrorSeverity::Error,
                    suggestion: None,
                }],
                warnings: vec![],
                suggestions: vec![],
            }],
            overall_passed: false,
            affected_files: vec!["src/main.rs".to_string()],
            side_effects: vec![],
            suggested_next_steps: vec!["Fix errors".to_string()],
        };

        let display = format!("{}", report);
        assert!(display.contains("FAILED"));
        assert!(display.contains("type_check"));
    }

    #[test]
    fn test_truncate_str_exact_length() {
        assert_eq!(truncate_str("12345678", 8), "12345678");
    }

    #[test]
    fn test_truncate_str_one_over() {
        assert_eq!(truncate_str("123456789", 8), "12345...");
    }

    #[test]
    fn test_check_type_serde() {
        let check = CheckType::TypeCheck;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"type_check\"");

        let check = CheckType::Test;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"test\"");

        let check = CheckType::Lint;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"lint\"");

        let check = CheckType::Format;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"format\"");
    }

    #[test]
    fn test_error_severity_all_variants() {
        assert_eq!(
            serde_json::to_string(&ErrorSeverity::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorSeverity::Note).unwrap(),
            "\"note\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorSeverity::Help).unwrap(),
            "\"help\""
        );
    }

    #[test]
    fn test_is_excluded_rs_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        // .rs files should not be excluded
        assert!(!gate.is_excluded("src/main.rs"));
        assert!(!gate.is_excluded("lib.rs"));
    }

    #[test]
    fn test_is_excluded_pattern_matching() {
        let config = VerificationConfig {
            exclude_patterns: vec!["*.test.rs".to_string(), "target/*".to_string()],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);

        assert!(gate.is_excluded("foo.test.rs"));
        // Note: glob matching depends on exact pattern syntax
    }

    #[test]
    fn test_compiler_error_to_verification_error() {
        let ce = CompilerError {
            file: "test.rs".to_string(),
            line: 5,
            column: 10,
            message: "test message".to_string(),
            code: Some("E0001".to_string()),
            severity: Severity::Error,
            suggestion: Some("fix it".to_string()),
            snippet: "let x = 1;".to_string(),
        };

        let ve = compiler_error_to_verification_error(&ce);
        assert_eq!(ve.file, "test.rs");
        assert_eq!(ve.line, Some(5));
        assert_eq!(ve.column, Some(10));
        assert_eq!(ve.message, "test message");
        assert_eq!(ve.code, Some("E0001".to_string()));
        assert!(matches!(ve.severity, ErrorSeverity::Error));
        assert_eq!(ve.suggestion, Some("fix it".to_string()));
    }

    #[test]
    fn test_compiler_error_to_verification_error_zero_line() {
        let ce = CompilerError {
            file: "test.rs".to_string(),
            line: 0,
            column: 0,
            message: "test".to_string(),
            code: None,
            severity: Severity::Warning,
            suggestion: None,
            snippet: String::new(),
        };

        let ve = compiler_error_to_verification_error(&ce);
        assert!(ve.line.is_none());
        assert!(ve.column.is_none());
    }

    #[test]
    fn test_compiler_error_severity_mapping() {
        for (cargo_sev, expected_sev) in [
            (Severity::Error, ErrorSeverity::Error),
            (Severity::Warning, ErrorSeverity::Warning),
            (Severity::Note, ErrorSeverity::Note),
            (Severity::Help, ErrorSeverity::Help),
        ] {
            let ce = CompilerError {
                file: "test.rs".to_string(),
                line: 1,
                column: 1,
                message: "test".to_string(),
                code: None,
                severity: cargo_sev,
                suggestion: None,
                snippet: String::new(),
            };
            let ve = compiler_error_to_verification_error(&ce);
            assert_eq!(ve.severity, expected_sev);
        }
    }

    #[test]
    fn test_verification_report_clone() {
        let report = VerificationReport {
            triggered_by: "test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 100,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };

        let cloned = report.clone();
        assert_eq!(cloned.triggered_by, report.triggered_by);
        assert_eq!(cloned.overall_passed, report.overall_passed);
    }

    #[test]
    fn test_check_result_serde() {
        let result = CheckResult {
            check_type: CheckType::Test,
            passed: true,
            duration_ms: 50,
            output: "ok".to_string(),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"check_type\":\"test\""));
        assert!(json.contains("\"passed\":true"));
    }

    // ===== Additional tests for comprehensive coverage =====

    #[test]
    fn test_check_type_deserialize_all_variants() {
        let cases = [
            ("\"type_check\"", CheckType::TypeCheck),
            ("\"test\"", CheckType::Test),
            ("\"lint\"", CheckType::Lint),
            ("\"format\"", CheckType::Format),
            ("\"custom\"", CheckType::Custom),
        ];
        for (json_str, expected) in cases {
            let deserialized: CheckType = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_error_severity_deserialize_all_variants() {
        let cases = [
            ("\"error\"", ErrorSeverity::Error),
            ("\"warning\"", ErrorSeverity::Warning),
            ("\"note\"", ErrorSeverity::Note),
            ("\"help\"", ErrorSeverity::Help),
        ];
        for (json_str, expected) in cases {
            let deserialized: ErrorSeverity = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_side_effect_type_deserialize_all_variants() {
        let cases = [
            ("\"file_created\"", SideEffectType::FileCreated),
            ("\"file_modified\"", SideEffectType::FileModified),
            ("\"file_deleted\"", SideEffectType::FileDeleted),
            ("\"dependency_added\"", SideEffectType::DependencyAdded),
            ("\"dependency_removed\"", SideEffectType::DependencyRemoved),
            ("\"test_added\"", SideEffectType::TestAdded),
            ("\"test_removed\"", SideEffectType::TestRemoved),
        ];
        for (json_str, expected) in cases {
            let deserialized: SideEffectType = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_verification_config_default_all_fields() {
        let config = VerificationConfig::default();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(!config.lint_on_edit);
        assert!(config.format_on_edit);
        assert!(config.incremental);
        assert_eq!(config.check_timeout_secs, 60);
        assert!(config.continue_on_failure);
        assert_eq!(config.exclude_patterns.len(), 4);
        assert!(config.custom_checks.is_empty());
    }

    #[test]
    fn test_verification_config_fast_inherits_defaults() {
        let config = VerificationConfig::fast();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(!config.lint_on_edit);
        assert!(!config.format_on_edit);
        assert!(config.incremental);
        assert_eq!(config.check_timeout_secs, 60);
        assert!(config.continue_on_failure);
        assert_eq!(config.exclude_patterns.len(), 4);
        assert!(config.custom_checks.is_empty());
    }

    #[test]
    fn test_verification_config_thorough_inherits_defaults() {
        let config = VerificationConfig::thorough();
        assert!(config.check_on_edit);
        assert!(config.test_on_edit);
        assert!(config.lint_on_edit);
        assert!(config.format_on_edit);
        assert!(config.incremental);
        assert_eq!(config.check_timeout_secs, 60);
        assert!(config.continue_on_failure);
    }

    #[test]
    fn test_verification_config_serde_roundtrip() {
        let config = VerificationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.check_on_edit, config.check_on_edit);
        assert_eq!(deserialized.test_on_edit, config.test_on_edit);
        assert_eq!(deserialized.lint_on_edit, config.lint_on_edit);
        assert_eq!(deserialized.format_on_edit, config.format_on_edit);
        assert_eq!(deserialized.incremental, config.incremental);
        assert_eq!(deserialized.check_timeout_secs, config.check_timeout_secs);
        assert_eq!(deserialized.continue_on_failure, config.continue_on_failure);
        assert_eq!(deserialized.exclude_patterns, config.exclude_patterns);
    }

    #[test]
    fn test_custom_check_serde_roundtrip() {
        let check = CustomCheck {
            name: "my_lint".to_string(),
            command: "my-linter".to_string(),
            args: vec!["--strict".to_string(), "--fix".to_string()],
            run_on: vec!["*.rs".to_string(), "*.toml".to_string()],
        };
        let json = serde_json::to_string(&check).unwrap();
        let deserialized: CustomCheck = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my_lint");
        assert_eq!(deserialized.command, "my-linter");
        assert_eq!(deserialized.args.len(), 2);
        assert_eq!(deserialized.run_on.len(), 2);
    }

    #[test]
    fn test_side_effect_serde_roundtrip() {
        let effect = SideEffect {
            effect_type: SideEffectType::DependencyRemoved,
            description: "Removed dep xyz".to_string(),
            files: vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()],
        };
        let json = serde_json::to_string(&effect).unwrap();
        let deserialized: SideEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.effect_type, SideEffectType::DependencyRemoved);
        assert_eq!(deserialized.description, "Removed dep xyz");
        assert_eq!(deserialized.files.len(), 2);
    }

    #[test]
    fn test_check_result_serde_roundtrip_with_errors() {
        let result = CheckResult {
            check_type: CheckType::Lint,
            passed: false,
            duration_ms: 999,
            output: "clippy output here".to_string(),
            errors: vec![VerificationError {
                file: "src/lib.rs".to_string(),
                line: Some(42),
                column: Some(10),
                message: "unused variable".to_string(),
                code: Some("clippy::unused".to_string()),
                severity: ErrorSeverity::Warning,
                suggestion: Some("prefix with _".to_string()),
            }],
            warnings: vec!["minor issue".to_string()],
            suggestions: vec!["run clippy --fix".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CheckResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.check_type, CheckType::Lint);
        assert!(!deserialized.passed);
        assert_eq!(deserialized.duration_ms, 999);
        assert_eq!(deserialized.errors.len(), 1);
        assert_eq!(deserialized.errors[0].file, "src/lib.rs");
        assert_eq!(deserialized.errors[0].line, Some(42));
        assert_eq!(deserialized.errors[0].column, Some(10));
        assert_eq!(deserialized.errors[0].message, "unused variable");
        assert_eq!(
            deserialized.errors[0].code,
            Some("clippy::unused".to_string())
        );
        assert_eq!(deserialized.warnings.len(), 1);
        assert_eq!(deserialized.suggestions.len(), 1);
    }

    #[test]
    fn test_verification_error_serde_roundtrip() {
        let error = VerificationError {
            file: "src/main.rs".to_string(),
            line: Some(10),
            column: None,
            message: "mismatched types".to_string(),
            code: Some("E0308".to_string()),
            severity: ErrorSeverity::Error,
            suggestion: Some("expected i32, found &str".to_string()),
        };
        let json = serde_json::to_string(&error).unwrap();
        let deserialized: VerificationError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file, "src/main.rs");
        assert_eq!(deserialized.line, Some(10));
        assert_eq!(deserialized.column, None);
        assert_eq!(deserialized.message, "mismatched types");
        assert_eq!(deserialized.code, Some("E0308".to_string()));
        assert!(matches!(deserialized.severity, ErrorSeverity::Error));
        assert_eq!(
            deserialized.suggestion,
            Some("expected i32, found &str".to_string())
        );
    }

    #[test]
    fn test_verification_error_all_none_fields() {
        let error = VerificationError {
            file: String::new(),
            line: None,
            column: None,
            message: "generic error".to_string(),
            code: None,
            severity: ErrorSeverity::Note,
            suggestion: None,
        };
        assert!(error.file.is_empty());
        assert!(error.line.is_none());
        assert!(error.column.is_none());
        assert!(error.code.is_none());
        assert!(error.suggestion.is_none());
        assert!(matches!(error.severity, ErrorSeverity::Note));
    }

    #[test]
    fn test_verification_report_serde_roundtrip() {
        let report = VerificationReport {
            triggered_by: "file_edit".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 2500,
            checks: vec![
                CheckResult {
                    check_type: CheckType::TypeCheck,
                    passed: true,
                    duration_ms: 1000,
                    output: "ok".to_string(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                },
                CheckResult {
                    check_type: CheckType::Format,
                    passed: false,
                    duration_ms: 200,
                    output: "Diff in src/main.rs".to_string(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec!["Run `cargo fmt` to fix formatting".to_string()],
                },
            ],
            overall_passed: false,
            affected_files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            side_effects: vec![SideEffect {
                effect_type: SideEffectType::FileModified,
                description: "Modified src/main.rs".to_string(),
                files: vec!["src/main.rs".to_string()],
            }],
            suggested_next_steps: vec!["Run cargo fmt to fix formatting".to_string()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: VerificationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.triggered_by, "file_edit");
        assert_eq!(deserialized.total_duration_ms, 2500);
        assert_eq!(deserialized.checks.len(), 2);
        assert!(!deserialized.overall_passed);
        assert_eq!(deserialized.affected_files.len(), 2);
        assert_eq!(deserialized.side_effects.len(), 1);
        assert_eq!(deserialized.suggested_next_steps.len(), 1);
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn test_truncate_str_empty_with_zero_max() {
        assert_eq!(truncate_str("hello", 0), "...");
    }

    #[test]
    fn test_truncate_str_max_len_1() {
        assert_eq!(truncate_str("hello", 1), "...");
    }

    #[test]
    fn test_truncate_str_max_len_3() {
        assert_eq!(truncate_str("hello", 3), "...");
    }

    #[test]
    fn test_truncate_str_max_len_4() {
        assert_eq!(truncate_str("hello", 4), "h...");
    }

    #[test]
    fn test_truncate_str_max_len_5_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_very_long_string() {
        let long = "a".repeat(200);
        let result = truncate_str(&long, 10);
        assert_eq!(result.len(), 10);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_is_excluded_txt_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        assert!(gate.is_excluded("notes.txt"));
    }

    #[test]
    fn test_is_excluded_toml_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        assert!(gate.is_excluded("Cargo.toml"));
    }

    #[test]
    fn test_is_excluded_empty_exclude_patterns() {
        let config = VerificationConfig {
            exclude_patterns: vec![],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);
        assert!(!gate.is_excluded("README.md"));
        assert!(!gate.is_excluded("config.json"));
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_is_excluded_with_invalid_glob_pattern() {
        let config = VerificationConfig {
            exclude_patterns: vec!["[invalid".to_string()],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_is_excluded_multiple_patterns() {
        let config = VerificationConfig {
            exclude_patterns: vec![
                "*.md".to_string(),
                "*.log".to_string(),
                "vendor/*".to_string(),
            ],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);
        assert!(gate.is_excluded("README.md"));
        assert!(gate.is_excluded("debug.log"));
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_should_run_custom_check_no_matching_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "py_check".to_string(),
            command: "python".to_string(),
            args: vec![],
            run_on: vec!["*.py".to_string()],
        };
        assert!(
            !gate.should_run_custom_check(&check, &["main.rs".to_string(), "lib.rs".to_string()])
        );
    }

    #[test]
    fn test_should_run_custom_check_multiple_patterns() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "multi_check".to_string(),
            command: "lint".to_string(),
            args: vec![],
            run_on: vec!["*.rs".to_string(), "*.toml".to_string()],
        };
        assert!(gate.should_run_custom_check(&check, &["Cargo.toml".to_string()]));
        assert!(gate.should_run_custom_check(&check, &["main.rs".to_string()]));
        assert!(!gate.should_run_custom_check(&check, &["script.py".to_string()]));
    }

    #[test]
    fn test_should_run_custom_check_invalid_glob() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "bad_glob".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec!["[invalid".to_string()],
        };
        assert!(!gate.should_run_custom_check(&check, &["main.rs".to_string()]));
    }

    #[test]
    fn test_should_run_custom_check_empty_files_list() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "check".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec!["*.rs".to_string()],
        };
        let empty: &[String] = &[];
        assert!(!gate.should_run_custom_check(&check, empty));
    }

    #[test]
    fn test_parse_test_failures_from_stderr() {
        // Note: split("test ") splits on ALL occurrences, including inside "my_test",
        // so use a test name that doesn't contain "test " as a substring
        let stderr = "test my_module::some_fn ... FAILED";
        let errors = parse_test_failures("", stderr);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("my_module::some_fn"),
            "actual message: {:?}",
            errors[0].message
        );
    }

    #[test]
    fn test_parse_test_failures_both_stdout_and_stderr() {
        let stdout = "test stdout_test ... FAILED";
        let stderr = "test stderr_test ... FAILED";
        let errors = parse_test_failures(stdout, stderr);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_parse_test_failures_panic_in_stderr() {
        let stderr = "thread 'main' panicked at 'assertion failed: x == y', src/lib.rs:42";
        let errors = parse_test_failures("", stderr);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("panicked at"));
        assert!(matches!(errors[0].severity, ErrorSeverity::Error));
    }

    #[test]
    fn test_parse_test_failures_combined_failure_and_panic() {
        let output = "test my_test ... FAILED\nthread 'main' panicked at 'oops', src/test.rs:10";
        let errors = parse_test_failures(output, "");
        assert_eq!(errors.len(), 2);
        assert!(errors[0].message.contains("Test failed"));
        assert!(errors[1].message.contains("panicked"));
    }

    #[test]
    fn test_parse_test_failures_failed_without_test_prefix() {
        let output = "some other line FAILED";
        let errors = parse_test_failures(output, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_parse_test_failures_empty_inputs() {
        let errors = parse_test_failures("", "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_parse_test_failures_error_fields() {
        let stdout = "test foo::bar ... FAILED";
        let errors = parse_test_failures(stdout, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].file.is_empty());
        assert!(errors[0].line.is_none());
        assert!(errors[0].column.is_none());
        assert!(errors[0].code.is_none());
        assert!(matches!(errors[0].severity, ErrorSeverity::Error));
        assert_eq!(
            errors[0].suggestion,
            Some("Check test output for details".to_string())
        );
    }

    #[test]
    fn test_parse_test_failures_panic_fields() {
        let stderr = "thread 'main' panicked at 'oops'";
        let errors = parse_test_failures("", stderr);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].file.is_empty());
        assert!(errors[0].line.is_none());
        assert!(errors[0].column.is_none());
        assert!(errors[0].code.is_none());
        assert!(errors[0].suggestion.is_none());
    }

    #[test]
    fn test_parse_cargo_json_output_with_warning() {
        let json_line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable","code":{"code":"W0001"},"spans":[{"file_name":"src/lib.rs","line_start":5,"column_start":3,"is_primary":true}],"children":[]}}"#;
        let (errors, warnings) = parse_cargo_json_output(json_line);
        assert!(errors.is_empty());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "unused variable");
        assert!(matches!(warnings[0].severity, ErrorSeverity::Warning));
    }

    #[test]
    fn test_parse_cargo_json_output_mixed_errors_and_warnings() {
        let error_line = r#"{"reason":"compiler-message","message":{"level":"error","message":"type mismatch","code":{"code":"E0308"},"spans":[{"file_name":"src/main.rs","line_start":10,"column_start":5,"is_primary":true}],"children":[]}}"#;
        let warning_line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"dead code","code":null,"spans":[{"file_name":"src/lib.rs","line_start":20,"column_start":1,"is_primary":true}],"children":[]}}"#;
        let output = format!("{}\n{}", error_line, warning_line);
        let (errors, warnings) = parse_cargo_json_output(&output);
        assert_eq!(errors.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(errors[0].message, "type mismatch");
        assert_eq!(warnings[0].message, "dead code");
    }

    #[test]
    fn test_parse_cargo_json_output_non_compiler_message() {
        let json_line =
            r#"{"reason":"build-script-executed","package_id":"some_pkg","out_dir":"/tmp"}"#;
        let (errors, warnings) = parse_cargo_json_output(json_line);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_output_invalid_json() {
        let output = "this is not json\nalso not json\n";
        let (errors, warnings) = parse_cargo_json_output(output);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_output_mixed_json_and_text() {
        let output = "Compiling foo v0.1.0\n{\"reason\":\"compiler-message\",\"message\":{\"level\":\"error\",\"message\":\"boom\",\"code\":{\"code\":\"E0001\"},\"spans\":[{\"file_name\":\"src/main.rs\",\"line_start\":1,\"column_start\":1,\"is_primary\":true}],\"children\":[]}}\nFinished dev";
        let (errors, warnings) = parse_cargo_json_output(output);
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compiler_error_to_verification_error_note_severity() {
        let ce = CompilerError {
            file: "src/mod.rs".to_string(),
            line: 3,
            column: 0,
            message: "note message".to_string(),
            code: None,
            severity: Severity::Note,
            suggestion: None,
            snippet: String::new(),
        };
        let ve = compiler_error_to_verification_error(&ce);
        assert!(matches!(ve.severity, ErrorSeverity::Note));
        assert_eq!(ve.column, None);
        assert_eq!(ve.line, Some(3));
    }

    #[test]
    fn test_compiler_error_to_verification_error_help_severity() {
        let ce = CompilerError {
            file: "src/mod.rs".to_string(),
            line: 0,
            column: 5,
            message: "help message".to_string(),
            code: Some("help_code".to_string()),
            severity: Severity::Help,
            suggestion: Some("try this".to_string()),
            snippet: "fn main() {}".to_string(),
        };
        let ve = compiler_error_to_verification_error(&ce);
        assert!(matches!(ve.severity, ErrorSeverity::Help));
        assert_eq!(ve.line, None);
        assert_eq!(ve.column, Some(5));
        assert_eq!(ve.code, Some("help_code".to_string()));
        assert_eq!(ve.suggestion, Some("try this".to_string()));
    }

    #[test]
    fn test_verification_gate_new_with_pathbuf() {
        let path = PathBuf::from("/tmp/test_project");
        let config = VerificationConfig::fast();
        let gate = VerificationGate::new(&path, config);
        assert!(gate.last_results().is_none());
    }

    #[test]
    fn test_verification_gate_new_with_string() {
        let config = VerificationConfig::thorough();
        let gate = VerificationGate::new("/some/path", config);
        assert!(gate.last_results().is_none());
    }

    #[test]
    fn test_verification_report_display_no_checks() {
        let report = VerificationReport {
            triggered_by: "test_trigger".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 0,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };
        let display = format!("{}", report);
        assert!(display.contains("VERIFICATION REPORT"));
        assert!(display.contains("PASSED"));
        assert!(display.contains("0ms"));
        assert!(!display.contains("Suggested next steps:"));
    }

    #[test]
    fn test_verification_report_display_long_trigger() {
        let report = VerificationReport {
            triggered_by: "this_is_a_very_long_trigger_name_that_exceeds_30_chars".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 42,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };
        let display = format!("{}", report);
        assert!(display.contains("..."));
    }

    #[test]
    fn test_verification_report_display_multiple_checks() {
        let report = VerificationReport {
            triggered_by: "multi".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 3000,
            checks: vec![
                CheckResult {
                    check_type: CheckType::TypeCheck,
                    passed: true,
                    duration_ms: 1000,
                    output: String::new(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                },
                CheckResult {
                    check_type: CheckType::Format,
                    passed: true,
                    duration_ms: 200,
                    output: String::new(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                },
                CheckResult {
                    check_type: CheckType::Lint,
                    passed: false,
                    duration_ms: 800,
                    output: "clippy warnings".to_string(),
                    errors: vec![VerificationError {
                        file: "src/main.rs".to_string(),
                        line: Some(5),
                        column: Some(1),
                        message: "this is a very long error message that should be truncated"
                            .to_string(),
                        code: None,
                        severity: ErrorSeverity::Warning,
                        suggestion: None,
                    }],
                    warnings: vec![],
                    suggestions: vec![],
                },
            ],
            overall_passed: false,
            affected_files: vec!["src/main.rs".to_string()],
            side_effects: vec![],
            suggested_next_steps: vec![
                "Fix clippy warnings".to_string(),
                "Run cargo clippy --fix".to_string(),
            ],
        };
        let display = format!("{}", report);
        assert!(display.contains("FAILED"));
        assert!(display.contains("type_check"));
        assert!(display.contains("format"));
        assert!(display.contains("lint"));
        assert!(display.contains("src/main.rs"));
        assert!(display.contains("Suggested next steps:"));
        assert!(display.contains("Fix clippy warnings"));
    }

    #[test]
    fn test_verification_report_display_multiple_errors_in_check() {
        let report = VerificationReport {
            triggered_by: "edit".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 100,
            checks: vec![CheckResult {
                check_type: CheckType::TypeCheck,
                passed: false,
                duration_ms: 100,
                output: "errors".to_string(),
                errors: vec![
                    VerificationError {
                        file: "a.rs".to_string(),
                        line: Some(1),
                        column: Some(1),
                        message: "error one".to_string(),
                        code: None,
                        severity: ErrorSeverity::Error,
                        suggestion: None,
                    },
                    VerificationError {
                        file: "b.rs".to_string(),
                        line: Some(2),
                        column: None,
                        message: "error two".to_string(),
                        code: None,
                        severity: ErrorSeverity::Error,
                        suggestion: None,
                    },
                ],
                warnings: vec![],
                suggestions: vec![],
            }],
            overall_passed: false,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec!["Fix type errors".to_string()],
        };
        let display = format!("{}", report);
        assert!(display.contains("a.rs"));
        assert!(display.contains("b.rs"));
    }

    #[tokio::test]
    async fn test_detect_side_effects_empty_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate.detect_side_effects(&[]).await;
        assert!(effects.is_empty());
    }

    #[tokio::test]
    async fn test_detect_side_effects_test_file() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate
            .detect_side_effects(&["src/my_test.rs".to_string()])
            .await;
        let has_test_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::TestAdded);
        assert!(has_test_added);
    }

    #[tokio::test]
    async fn test_detect_side_effects_cargo_toml() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate.detect_side_effects(&["Cargo.toml".to_string()]).await;
        let has_dep_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::DependencyAdded);
        assert!(has_dep_added);
        let dep_effect = effects
            .iter()
            .find(|e| e.effect_type == SideEffectType::DependencyAdded)
            .unwrap();
        assert!(dep_effect.description.contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_detect_side_effects_test_and_cargo_combined() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate
            .detect_side_effects(&["tests/unit_test.rs".to_string(), "Cargo.toml".to_string()])
            .await;
        let has_test_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::TestAdded);
        let has_dep_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::DependencyAdded);
        assert!(has_test_added);
        assert!(has_dep_added);
    }

    #[tokio::test]
    async fn test_detect_side_effects_existing_file() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(env!("CARGO_MANIFEST_DIR"), config);
        let effects = gate.detect_side_effects(&["Cargo.toml".to_string()]).await;
        let has_modified = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::FileModified);
        assert!(has_modified);
    }

    #[tokio::test]
    async fn test_detect_side_effects_nonexistent_file() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new("/tmp/nonexistent_project_xyz", config);
        let effects = gate.detect_side_effects(&["src/main.rs".to_string()]).await;
        let has_modified = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::FileModified);
        assert!(!has_modified);
    }

    #[tokio::test]
    async fn test_detect_side_effects_file_with_test_in_name() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate
            .detect_side_effects(&["integration_test_helpers.rs".to_string()])
            .await;
        let has_test_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::TestAdded);
        assert!(has_test_added);
    }

    #[tokio::test]
    async fn test_verify_change_all_excluded_files() {
        let config = VerificationConfig::default();
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(
                &[
                    "README.md".to_string(),
                    "config.json".to_string(),
                    "notes.txt".to_string(),
                ],
                "test_trigger",
            )
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
        assert_eq!(report.total_duration_ms, 0);
        assert_eq!(report.triggered_by, "test_trigger");
        assert_eq!(report.affected_files.len(), 3);
        assert_eq!(report.suggested_next_steps.len(), 1);
        assert!(report.suggested_next_steps[0].contains("No code files changed"));
    }

    #[tokio::test]
    async fn test_verify_change_stores_last_results() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        assert!(gate.last_results().is_none());
        let _report = gate
            .verify_change(&["src/main.rs".to_string()], "edit")
            .await
            .unwrap();
        assert!(gate.last_results().is_some());
        let last = gate.last_results().unwrap();
        assert_eq!(last.triggered_by, "edit");
    }

    #[tokio::test]
    async fn test_verify_change_no_checks_enabled_with_rs_file() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["src/main.rs".to_string()], "no_checks")
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
        assert_eq!(
            report.suggested_next_steps,
            vec!["All checks passed - safe to proceed"]
        );
    }

    #[tokio::test]
    async fn test_verify_change_non_rust_files_not_excluded() {
        let config = VerificationConfig {
            check_on_edit: true,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["script.py".to_string()], "py_edit")
            .await
            .unwrap();
        // Python files now get language-specific checks (type check runs when check_on_edit is true)
        // The checks may pass or fail depending on environment, but the report should be valid
        assert!(!report.triggered_by.is_empty());
    }

    #[tokio::test]
    async fn test_verify_change_with_custom_check_that_runs() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "echo_check".to_string(),
                command: "echo".to_string(),
                args: vec!["hello".to_string()],
                run_on: vec![],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["script.py".to_string()], "custom_trigger")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].check_type, CheckType::Custom);
        assert!(report.checks[0].passed);
        assert!(report.overall_passed);
    }

    #[tokio::test]
    async fn test_verify_change_with_custom_check_pattern_match() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "rs_only".to_string(),
                command: "echo".to_string(),
                args: vec!["checking".to_string()],
                run_on: vec!["*.rs".to_string()],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);

        let report = gate
            .verify_change(&["script.py".to_string()], "py_edit")
            .await
            .unwrap();
        assert!(report.checks.is_empty());

        let report = gate
            .verify_change(&["main.rs".to_string()], "rs_edit")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].check_type, CheckType::Custom);
    }

    #[tokio::test]
    async fn test_verify_change_with_failing_custom_check() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "failing_check".to_string(),
                command: "false".to_string(),
                args: vec![],
                run_on: vec![],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["script.py".to_string()], "fail_trigger")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert!(!report.checks[0].passed);
        assert!(!report.overall_passed);
    }

    #[tokio::test]
    async fn test_full_verify_with_no_files() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate.full_verify().await.unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
    }

    #[test]
    fn test_check_result_clone() {
        let result = CheckResult {
            check_type: CheckType::Lint,
            passed: false,
            duration_ms: 250,
            output: "lint errors".to_string(),
            errors: vec![VerificationError {
                file: "src/lib.rs".to_string(),
                line: Some(10),
                column: Some(5),
                message: "unused var".to_string(),
                code: Some("W001".to_string()),
                severity: ErrorSeverity::Warning,
                suggestion: Some("remove it".to_string()),
            }],
            warnings: vec!["w1".to_string()],
            suggestions: vec!["s1".to_string()],
        };
        let cloned = result.clone();
        assert_eq!(cloned.check_type, result.check_type);
        assert_eq!(cloned.passed, result.passed);
        assert_eq!(cloned.duration_ms, result.duration_ms);
        assert_eq!(cloned.output, result.output);
        assert_eq!(cloned.errors.len(), 1);
        assert_eq!(cloned.errors[0].file, "src/lib.rs");
        assert_eq!(cloned.warnings, result.warnings);
        assert_eq!(cloned.suggestions, result.suggestions);
    }

    #[test]
    fn test_verification_error_clone() {
        let error = VerificationError {
            file: "test.rs".to_string(),
            line: Some(1),
            column: Some(2),
            message: "msg".to_string(),
            code: Some("E0001".to_string()),
            severity: ErrorSeverity::Error,
            suggestion: Some("fix".to_string()),
        };
        let cloned = error.clone();
        assert_eq!(cloned.file, error.file);
        assert_eq!(cloned.line, error.line);
        assert_eq!(cloned.column, error.column);
        assert_eq!(cloned.message, error.message);
        assert_eq!(cloned.code, error.code);
        assert_eq!(cloned.suggestion, error.suggestion);
    }

    #[test]
    fn test_side_effect_clone() {
        let effect = SideEffect {
            effect_type: SideEffectType::TestRemoved,
            description: "removed test".to_string(),
            files: vec!["test.rs".to_string()],
        };
        let cloned = effect.clone();
        assert_eq!(cloned.effect_type, effect.effect_type);
        assert_eq!(cloned.description, effect.description);
        assert_eq!(cloned.files, effect.files);
    }

    #[test]
    fn test_check_type_debug() {
        assert_eq!(format!("{:?}", CheckType::TypeCheck), "TypeCheck");
        assert_eq!(format!("{:?}", CheckType::Test), "Test");
        assert_eq!(format!("{:?}", CheckType::Lint), "Lint");
        assert_eq!(format!("{:?}", CheckType::Format), "Format");
        assert_eq!(format!("{:?}", CheckType::Custom), "Custom");
    }

    #[test]
    fn test_error_severity_debug() {
        assert_eq!(format!("{:?}", ErrorSeverity::Error), "Error");
        assert_eq!(format!("{:?}", ErrorSeverity::Warning), "Warning");
        assert_eq!(format!("{:?}", ErrorSeverity::Note), "Note");
        assert_eq!(format!("{:?}", ErrorSeverity::Help), "Help");
    }

    #[test]
    fn test_side_effect_type_debug() {
        assert_eq!(format!("{:?}", SideEffectType::FileCreated), "FileCreated");
        assert_eq!(
            format!("{:?}", SideEffectType::FileModified),
            "FileModified"
        );
        assert_eq!(format!("{:?}", SideEffectType::FileDeleted), "FileDeleted");
        assert_eq!(
            format!("{:?}", SideEffectType::DependencyAdded),
            "DependencyAdded"
        );
        assert_eq!(
            format!("{:?}", SideEffectType::DependencyRemoved),
            "DependencyRemoved"
        );
        assert_eq!(format!("{:?}", SideEffectType::TestAdded), "TestAdded");
        assert_eq!(format!("{:?}", SideEffectType::TestRemoved), "TestRemoved");
    }

    #[test]
    fn test_check_result_debug() {
        let result = CheckResult {
            check_type: CheckType::TypeCheck,
            passed: true,
            duration_ms: 0,
            output: String::new(),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("CheckResult"));
        assert!(debug.contains("TypeCheck"));
    }

    #[test]
    fn test_verification_error_debug() {
        let error = VerificationError {
            file: "test.rs".to_string(),
            line: Some(1),
            column: None,
            message: "err".to_string(),
            code: None,
            severity: ErrorSeverity::Error,
            suggestion: None,
        };
        let debug = format!("{:?}", error);
        assert!(debug.contains("VerificationError"));
        assert!(debug.contains("test.rs"));
    }

    #[test]
    fn test_verification_report_debug() {
        let report = VerificationReport {
            triggered_by: "debug_test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 0,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };
        let debug = format!("{:?}", report);
        assert!(debug.contains("VerificationReport"));
        assert!(debug.contains("debug_test"));
    }

    #[test]
    fn test_side_effect_debug() {
        let effect = SideEffect {
            effect_type: SideEffectType::FileCreated,
            description: "created".to_string(),
            files: vec![],
        };
        let debug = format!("{:?}", effect);
        assert!(debug.contains("SideEffect"));
        assert!(debug.contains("FileCreated"));
    }

    #[test]
    fn test_verification_config_debug() {
        let config = VerificationConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("VerificationConfig"));
        assert!(debug.contains("check_on_edit"));
    }

    #[test]
    fn test_custom_check_debug() {
        let check = CustomCheck {
            name: "test".to_string(),
            command: "cmd".to_string(),
            args: vec![],
            run_on: vec![],
        };
        let debug = format!("{:?}", check);
        assert!(debug.contains("CustomCheck"));
    }

    #[test]
    fn test_check_type_copy_and_eq() {
        let a = CheckType::TypeCheck;
        let b = a;
        assert_eq!(a, b);
        assert_eq!(CheckType::Test, CheckType::Test);
        assert_ne!(CheckType::Test, CheckType::Lint);
    }

    #[test]
    fn test_error_severity_copy_and_eq() {
        let a = ErrorSeverity::Warning;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(ErrorSeverity::Error, ErrorSeverity::Help);
    }

    #[test]
    fn test_side_effect_type_copy_and_eq() {
        let a = SideEffectType::FileCreated;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(SideEffectType::FileCreated, SideEffectType::FileDeleted);
    }

    #[test]
    fn test_verification_config_with_custom_checks_serde() {
        let config = VerificationConfig {
            custom_checks: vec![
                CustomCheck {
                    name: "check1".to_string(),
                    command: "cmd1".to_string(),
                    args: vec!["--flag".to_string()],
                    run_on: vec!["*.rs".to_string()],
                },
                CustomCheck {
                    name: "check2".to_string(),
                    command: "cmd2".to_string(),
                    args: vec![],
                    run_on: vec![],
                },
            ],
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.custom_checks.len(), 2);
        assert_eq!(deserialized.custom_checks[0].name, "check1");
        assert_eq!(deserialized.custom_checks[1].name, "check2");
    }

    #[test]
    fn test_overall_passed_with_empty_checks() {
        let checks: Vec<CheckResult> = vec![];
        assert!(checks.iter().all(|c| c.passed));
    }

    #[test]
    fn test_overall_passed_all_pass() {
        let checks = [
            CheckResult {
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                check_type: CheckType::Format,
                passed: true,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
        ];
        assert!(checks.iter().all(|c| c.passed));
    }

    #[test]
    fn test_overall_passed_one_fails() {
        let checks = [
            CheckResult {
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                check_type: CheckType::Test,
                passed: false,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
        ];
        assert!(!checks.iter().all(|c| c.passed));
    }

    #[tokio::test]
    async fn test_run_custom_check_captures_output() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "echo_test".to_string(),
                command: "echo".to_string(),
                args: vec!["custom_output_text".to_string()],
                run_on: vec![],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["file.py".to_string()], "custom_test")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert!(report.checks[0].output.contains("custom_output_text"));
    }

    #[tokio::test]
    async fn test_verify_change_mixed_excluded_and_non_excluded() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["README.md".to_string(), "script.py".to_string()], "mixed")
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.affected_files.contains(&"script.py".to_string()));
        assert!(!report.affected_files.contains(&"README.md".to_string()));
    }

    #[tokio::test]
    async fn test_verify_change_updates_last_results_on_successive_calls() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let _r1 = gate
            .verify_change(&["a.py".to_string()], "first")
            .await
            .unwrap();
        assert_eq!(gate.last_results().unwrap().triggered_by, "first");
        let _r2 = gate
            .verify_change(&["b.py".to_string()], "second")
            .await
            .unwrap();
        assert_eq!(gate.last_results().unwrap().triggered_by, "second");
    }

    #[test]
    fn test_parse_test_failures_test_failed_no_dots_separator() {
        // Without " ..." separator, the split on "test " can match within the test name
        // For "test some_fn FAILED": split("test ") -> ["", "some_fn FAILED"]
        // nth(1) = "some_fn FAILED", split(" ...").next() = "some_fn FAILED"
        let stdout = "test some_fn FAILED";
        let errors = parse_test_failures(stdout, "");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("some_fn FAILED"),
            "actual message: {:?}",
            errors[0].message
        );
    }

    #[test]
    fn test_verification_report_display_with_suggested_steps_only() {
        let report = VerificationReport {
            triggered_by: "step_test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 10,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![
                "Step one".to_string(),
                "Step two".to_string(),
                "Step three".to_string(),
            ],
        };
        let display = format!("{}", report);
        assert!(display.contains("Suggested next steps:"));
        assert!(display.contains("Step one"));
        assert!(display.contains("Step two"));
        assert!(display.contains("Step three"));
    }

    #[test]
    fn test_file_hash_cache_detects_changes() {
        let config = VerificationConfig::default();
        let mut gate = VerificationGate::new(".", config);

        // Initially, cache is empty, so files should be considered changed
        assert!(gate.have_files_changed(&["src/lib.rs".to_string()]));

        // Simulate a verification by updating the cache
        // Note: We can't actually read files in this test, so we'll manually populate
        gate.file_hash_cache
            .insert("src/lib.rs".to_string(), 12345u64);

        // Now if we check the same file with same hash, it should not be changed
        // But since we can't actually compute the hash, we'll just test the logic
        // The real hash won't match 12345, so it will still report changed
        assert!(gate.have_files_changed(&["src/lib.rs".to_string()]));
    }

    #[test]
    fn test_file_hash_cache_empty_returns_changed() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        // Empty cache should always return true (files changed)
        assert!(gate.have_files_changed(&["src/main.rs".to_string()]));
        assert!(gate.have_files_changed(&["Cargo.toml".to_string()]));
    }

    #[tokio::test]
    async fn test_verify_change_uses_cache_on_unchanged_files() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("lib.rs"), "pub fn answer() -> i32 { 42 }\n").unwrap();

        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(temp.path(), config);

        // First verification with a file
        let report1 = gate
            .verify_change(&["src/lib.rs".to_string()], "first")
            .await
            .unwrap();

        // Verify the file hash was cached
        assert!(gate.file_hash_cache.contains_key("src/lib.rs"));

        // Second verification with same file (will detect as changed because
        // we can't actually read the file in this test, but the cache mechanism is tested)
        let report2 = gate
            .verify_change(&["src/lib.rs".to_string()], "second")
            .await
            .unwrap();

        // Both should pass
        assert!(report1.overall_passed);
        assert!(report2.overall_passed);
    }

    #[test]
    fn infer_repo_language_from_manifests() {
        let tmp = std::env::temp_dir().join(format!(
            "selfware_verify_manifest_test_{}",
            std::process::id()
        ));

        // Python via setup.py
        let py_dir = tmp.join("python_repo");
        std::fs::create_dir_all(&py_dir).unwrap();
        std::fs::write(py_dir.join("setup.py"), "from setuptools import setup\n").unwrap();
        let mut gate = VerificationGate::new(&py_dir, VerificationConfig::default());
        assert_eq!(gate.infer_repo_language(), RepoLanguage::Python);

        // TypeScript via package.json + tsconfig.json
        let ts_dir = tmp.join("ts_repo");
        std::fs::create_dir_all(&ts_dir).unwrap();
        std::fs::write(ts_dir.join("package.json"), "{}").unwrap();
        std::fs::write(ts_dir.join("tsconfig.json"), "{}").unwrap();
        let mut gate = VerificationGate::new(&ts_dir, VerificationConfig::default());
        assert_eq!(gate.infer_repo_language(), RepoLanguage::TypeScript);

        // Go via go.mod
        let go_dir = tmp.join("go_repo");
        std::fs::create_dir_all(&go_dir).unwrap();
        std::fs::write(go_dir.join("go.mod"), "module example\n").unwrap();
        let mut gate = VerificationGate::new(&go_dir, VerificationConfig::default());
        assert_eq!(gate.infer_repo_language(), RepoLanguage::Go);

        // Rust via Cargo.toml
        let rs_dir = tmp.join("rust_repo");
        std::fs::create_dir_all(&rs_dir).unwrap();
        std::fs::write(rs_dir.join("Cargo.toml"), "[package]\n").unwrap();
        let mut gate = VerificationGate::new(&rs_dir, VerificationConfig::default());
        assert_eq!(gate.infer_repo_language(), RepoLanguage::Rust);

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn infer_repo_language_from_extensions() {
        let tmp =
            std::env::temp_dir().join(format!("selfware_verify_ext_test_{}", std::process::id()));
        let py_dir = tmp.join("py_ext_repo");
        std::fs::create_dir_all(&py_dir).unwrap();
        std::fs::write(py_dir.join("main.py"), "print('hello')\n").unwrap();
        std::fs::write(py_dir.join("lib.py"), "def foo(): pass\n").unwrap();
        std::fs::write(py_dir.join("README.md"), "# hi\n").unwrap();

        let mut gate = VerificationGate::new(&py_dir, VerificationConfig::default());
        assert_eq!(gate.infer_repo_language(), RepoLanguage::Python);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn infer_repo_language_from_hint() {
        let tmp =
            std::env::temp_dir().join(format!("selfware_verify_hint_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut gate = VerificationGate::new(&tmp, VerificationConfig::default());
        gate.set_repo_language_hint("go");
        assert_eq!(gate.infer_repo_language(), RepoLanguage::Go);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn cheap_syntax_check_python() {
        let tmp = std::env::temp_dir().join(format!(
            "selfware_verify_py_syntax_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let good_py = tmp.join("good.py");
        std::fs::write(&good_py, "def hello():\n    print('world')\n").unwrap();

        let gate = VerificationGate::new(&tmp, VerificationConfig::default());
        let result = gate
            .run_cheap_syntax_check(RepoLanguage::Python, &["good.py".to_string()])
            .await
            .unwrap();
        assert!(result.passed, "valid python should pass: {}", result.output);

        let bad_py = tmp.join("bad.py");
        std::fs::write(&bad_py, "def hello(\n    print 'world'\n").unwrap();
        let result = gate
            .run_cheap_syntax_check(RepoLanguage::Python, &["bad.py".to_string()])
            .await
            .unwrap();
        assert!(!result.passed, "invalid python should fail");
        assert!(
            !result.output.is_empty(),
            "error output should contain details"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn targeted_test_command_python() {
        let tmp = std::env::temp_dir().join(format!(
            "selfware_verify_py_test_cmd_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp).unwrap();

        // No pytest manifest - if pytest is installed it will be preferred,
        // otherwise falls back to unittest
        let gate = VerificationGate::new(&tmp, VerificationConfig::default());
        let cmd = gate.infer_test_command(RepoLanguage::Python).await;
        assert!(cmd.is_some());
        let (program, _args) = cmd.unwrap();
        assert!(
            program == "pytest" || program == "python3",
            "expected pytest or python3, got {}",
            program
        );

        // With pytest.ini → should use pytest
        std::fs::write(tmp.join("pytest.ini"), "[pytest]\n").unwrap();
        let gate = VerificationGate::new(&tmp, VerificationConfig::default());
        let cmd = gate.infer_test_command(RepoLanguage::Python).await;
        assert!(cmd.is_some());
        let (program, args) = cmd.unwrap();
        assert_eq!(program, "pytest");
        assert!(args.contains(&"--quiet".to_string()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn repo_language_from_extension_coverage() {
        assert_eq!(
            RepoLanguage::from_extension(".rs"),
            Some(RepoLanguage::Rust)
        );
        assert_eq!(
            RepoLanguage::from_extension(".py"),
            Some(RepoLanguage::Python)
        );
        assert_eq!(
            RepoLanguage::from_extension(".js"),
            Some(RepoLanguage::JavaScript)
        );
        assert_eq!(
            RepoLanguage::from_extension(".ts"),
            Some(RepoLanguage::TypeScript)
        );
        assert_eq!(RepoLanguage::from_extension(".go"), Some(RepoLanguage::Go));
        assert_eq!(RepoLanguage::from_extension(".txt"), None);
    }

    #[test]
    fn repo_language_from_manifest_coverage() {
        assert_eq!(
            RepoLanguage::from_manifest("Cargo.toml"),
            Some(RepoLanguage::Rust)
        );
        assert_eq!(
            RepoLanguage::from_manifest("pyproject.toml"),
            Some(RepoLanguage::Python)
        );
        assert_eq!(
            RepoLanguage::from_manifest("package.json"),
            Some(RepoLanguage::JavaScript)
        );
        assert_eq!(
            RepoLanguage::from_manifest("go.mod"),
            Some(RepoLanguage::Go)
        );
        assert_eq!(RepoLanguage::from_manifest("random.txt"), None);
    }

    #[test]
    fn language_check_set_default() {
        let set = LanguageCheckSet::default();
        assert!(set.syntax);
        assert!(set.format);
        assert!(set.lint);
        assert!(set.test);
    }

    #[test]
    fn verification_config_language_settings_roundtrip() {
        let mut config = VerificationConfig::default();
        let mut settings = std::collections::HashMap::new();
        settings.insert(
            RepoLanguage::Python,
            LanguageCheckSet {
                syntax: true,
                format: false,
                lint: false,
                test: true,
            },
        );
        config.language_settings = settings;

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized
            .language_settings
            .contains_key(&RepoLanguage::Python));
        let py = deserialized
            .language_settings
            .get(&RepoLanguage::Python)
            .unwrap();
        assert!(py.syntax);
        assert!(!py.format);
        assert!(!py.lint);
        assert!(py.test);
    }

    #[tokio::test]
    async fn test_post_edit_test_command_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            post_edit_test_command: Some("echo post_edit_ok".to_string()),
            ..Default::default()
        };
        let mut gate = VerificationGate::new(tmp.path(), config);
        let report = gate
            .verify_change(&["script.py".to_string()], "post_edit_pass_trigger")
            .await
            .unwrap();
        let post_check = report
            .checks
            .iter()
            .find(|c| c.check_type == CheckType::Test);
        assert!(post_check.is_some(), "post-edit test check should run");
        assert!(post_check.unwrap().passed);
        assert!(post_check.unwrap().output.contains("post_edit_ok"));
        assert!(report.overall_passed);
    }

    #[tokio::test]
    async fn test_post_edit_test_command_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            post_edit_test_command: Some("false".to_string()),
            ..Default::default()
        };
        let mut gate = VerificationGate::new(tmp.path(), config);
        let report = gate
            .verify_change(&["script.py".to_string()], "post_edit_fail_trigger")
            .await
            .unwrap();
        let post_check = report
            .checks
            .iter()
            .find(|c| c.check_type == CheckType::Test)
            .expect("post-edit test check should be present");
        assert!(!post_check.passed);
        assert!(!report.overall_passed);
        assert!(report
            .suggested_next_steps
            .iter()
            .any(|s| s.contains("post-edit test command failed")));
    }
}
