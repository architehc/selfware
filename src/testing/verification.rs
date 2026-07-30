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
        // Do NOT delegate with an empty file list: verify_change early-returns
        // overall_passed=true with zero checks for empty input, so a "full
        // verification" would pass vacuously without running anything. Run the
        // crate-wide checks the config enables.
        let start = Instant::now();
        let mut checks = Vec::new();
        if self.config.check_on_edit {
            checks.push(self.run_cargo_check().await?);
        }
        if self.config.lint_on_edit {
            checks.push(self.run_cargo_clippy().await?);
        }
        if self.config.test_on_edit {
            checks.push(self.run_cargo_test().await?);
        }
        let overall_passed = checks.iter().all(|c| c.passed);
        let suggested_next_steps = if checks.is_empty() {
            vec!["No verification checks enabled in config".to_string()]
        } else {
            vec![]
        };
        Ok(VerificationReport {
            triggered_by: "full_verification".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: start.elapsed().as_millis() as u64,
            overall_passed,
            affected_files: vec![],
            side_effects: vec![],
            checks,
            suggested_next_steps,
        })
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

pub(crate) fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // floor_char_boundary: byte-slicing at max_len-3 panics when a
        // multi-byte char straddles the cut (dry_run's twin already does this).
        let end = s.floor_char_boundary(max_len.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
#[path = "../../tests/unit/testing/verification/verification_test.rs"]
mod tests;
