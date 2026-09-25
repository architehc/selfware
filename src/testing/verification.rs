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
///
/// `success` means the run ACTUALLY COMPLETED: the child exited
/// successfully AND its output was fully collected within the deadline.
/// Whenever `timed_out` is set the verdict is fail-closed — `success` is
/// false even if the parent exited 0, because the invocation was killed and
/// its output never fully collected. Consumers gate on `success` alone.
struct ReapedOutput {
    success: bool,
    timed_out: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Run a verification command with a timeout, capturing output and reaping the
/// ENTIRE process group on timeout. A hung `cargo check` (or the `rustc`
/// children it spawns) would otherwise stall the agent forever and leave
/// orphaned processes holding `target/` locks — a self-reinforcing stall for
/// unattended runs.
///
/// The child's environment is SANITIZED first (see
/// `crate::safety::process_env`): verification executes project-controlled
/// programs/linters, and an unsanitized child would inherit every credential
/// on the box (`SELFWARE_API_KEY`, `AWS_*`, …). No verification command is
/// credential-mediated, so nothing is preserved.
async fn run_reaped(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout_secs: u64,
) -> Result<ReapedOutput> {
    run_reaped_args(program, args.iter().copied(), cwd, timeout_secs).await
}

/// Generic form of [`run_reaped`] accepting any args collection (e.g. a
/// `Vec<String>` parsed from a config command string).
async fn run_reaped_args<I, S>(
    program: &str,
    args: I,
    cwd: &Path,
    timeout_secs: u64,
) -> Result<ReapedOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    run_reaped_env(program, args, cwd, timeout_secs, &[]).await
}

/// [`run_reaped_args`] plus task-specific environment variables, set AFTER
/// the sanitizer clears the inherited environment (e.g.
/// `PYTHONDONTWRITEBYTECODE=1` for the Python syntax check).
async fn run_reaped_env<I, S>(
    program: &str,
    args: I,
    cwd: &Path,
    timeout_secs: u64,
    env: &[(&str, &str)],
) -> Result<ReapedOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    use tokio::io::AsyncReadExt;

    let mut cmd = Command::new(program);
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.envs(env.iter().copied());
    cmd.kill_on_drop(true);
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
    let mut so_task = tokio::spawn(async move {
        let mut b = Vec::new();
        if let Some(ref mut s) = so {
            let _ = s.read_to_end(&mut b).await;
        }
        b
    });
    let mut se_task = tokio::spawn(async move {
        let mut b = Vec::new();
        if let Some(ref mut s) = se {
            let _ = s.read_to_end(&mut b).await;
        }
        b
    });

    let timeout = tokio::time::Duration::from_secs(timeout_secs.max(1));
    // ONE absolute deadline covers BOTH the wait and the output collection:
    // a descendant that retained a pipe keeps it open even after the group
    // was reaped, so an unbounded collection would stall verification
    // indefinitely past its timeout (review finding: QA hang beyond timeout).
    let deadline = std::time::Instant::now() + timeout;
    let (success, wait_timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
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

    // Bound the collection phase with the SAME absolute deadline; on
    // collection timeout re-kill the group and abort only the drains that
    // never finished. Each drain has its own result slot (shared helper with
    // `run_command_bounded`), so output one stream ALREADY captured survives
    // a timeout of the other instead of being discarded with it.
    let (stdout, mut stderr, drain_timed_out) = {
        use crate::tools::process_guard::{collect_drains_until, DRAIN_AFTER_KILL};
        let mut so_slot: Option<Vec<u8>> = None;
        let mut se_slot: Option<Vec<u8>> = None;
        collect_drains_until(
            tokio::time::Instant::from_std(deadline),
            (&mut so_task, &mut so_slot),
            (&mut se_task, &mut se_slot),
        )
        .await;
        let drain_timed_out = so_slot.is_none() || se_slot.is_none();
        if drain_timed_out {
            #[cfg(unix)]
            if let Some(p) = pid {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(p as i32), Signal::SIGKILL);
            }
            collect_drains_until(
                tokio::time::Instant::now() + DRAIN_AFTER_KILL,
                (&mut so_task, &mut so_slot),
                (&mut se_task, &mut se_slot),
            )
            .await;
            if so_slot.is_none() {
                so_task.abort();
            }
            if se_slot.is_none() {
                se_task.abort();
            }
        }
        (
            so_slot.unwrap_or_default(),
            se_slot.unwrap_or_default(),
            drain_timed_out,
        )
    };
    let timed_out = wait_timed_out || drain_timed_out;
    // Fail-closed: a timed-out run (wait OR collection) never reports
    // success, even when the parent exited 0 — the invocation was killed and
    // its output was never fully collected. success therefore means "the run
    // actually completed"; consumers (check/fmt/clippy/custom gates) derive
    // their verdict from `success` alone and must not see a timeout as green.
    let success = success && !timed_out;
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
        timed_out,
        stdout,
        stderr,
    })
}

/// Verification result for a single check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_type: CheckType,
    /// Whether the check blocks. A check that could NOT run (`not_run`) is
    /// non-blocking and therefore `passed`, but it verified nothing: read
    /// `not_run` before presenting a pass.
    pub passed: bool,
    /// The check did not run at all (tool missing, could not be started, or
    /// the host toolchain cannot judge this project's language level). It
    /// asserts nothing about the code: neither a pass nor a failure
    /// (AGENTS.md Rule 3). `output`/`warnings` say why.
    #[serde(default)]
    pub not_run: bool,
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
            Self::JavaScript => &[".js", ".jsx", ".mjs", ".cjs"],
            Self::TypeScript => &[".ts", ".tsx", ".mts", ".cts"],
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
            ".js" | ".jsx" | ".mjs" | ".cjs" => Some(Self::JavaScript),
            ".ts" | ".tsx" | ".mts" | ".cts" => Some(Self::TypeScript),
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
    /// Working directory for tool executions (defaults to process cwd).
    working_dir: Option<PathBuf>,
}

/// Verdict on a non-zero `rustfmt --check` run. rustfmt exits 1 both for
/// genuine parse failures and for formatting differences, so the cheap Rust
/// "syntax" check must classify by OUTPUT, not by exit code. The split fixes
/// a false negative (finding C, review item #5): a run over unformatted-but-
/// valid Rust failed the syntax gate ("Fix Rust syntax errors") and marked
/// verification FAILED even though every test passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RustfmtFailureKind {
    /// rustfmt reported formatting differences only (`Diff in ...` hunks)
    /// with no error-shaped lines: the code is syntactically valid. The
    /// result is an advisory format note, not a blocking syntax failure.
    FormattingDiff,
    /// The rustfmt tool never ran: a rustup shim answered that the toolchain
    /// has no rustfmt component ("error: toolchain 'X' does not have
    /// component 'rustfmt'", "error: rustfmt is not installed for the
    /// toolchain 'X'"). A missing tool is check-NOT-run — it asserts nothing
    /// about the code and must never block as a syntax failure (W7b finding
    /// 4: the error-line arm used to catch the shim's `error:` prefix).
    ToolUnavailable,
    /// Genuine parse error, operational error (missing/unreadable file), or
    /// any failure that does not match the pure-diff shape. Fail-closed:
    /// never turn a real syntax error green.
    SyntaxFailure,
}

/// Classify a failed `rustfmt --check` run from its combined output.
///
/// A rustup-shim "component not installed" output is
/// [`RustfmtFailureKind::ToolUnavailable`] (checked FIRST — the tool never
/// ran, so neither diff nor parse analysis applies). Only a run whose output
/// consists PURELY of `Diff in ...` formatting hunks (no `error:`/`Error:`
/// lines — rustfmt prints parse errors to its stderr, and an
/// unreadable/missing file prints `Error: ...`) classifies as
/// [`RustfmtFailureKind::FormattingDiff`]. Any error-shaped line (a mixed run
/// with a parse error in one file and diffs in another counts as an error) or
/// any unclassifiable failure stays a blocking `SyntaxFailure`.
pub(crate) fn classify_rustfmt_failure(combined: &str) -> RustfmtFailureKind {
    if rustfmt_output_is_tool_unavailable(combined) {
        return RustfmtFailureKind::ToolUnavailable;
    }
    let saw_diff = combined
        .lines()
        .any(|l| l.trim_start().starts_with("Diff in "));
    // Diff hunks always prefix changed lines with `+`/`-`, so a trimmed line
    // starting with an error marker can only be real error output, never a
    // source line copied into the diff.
    let saw_error_line = combined.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("error:")
            || t.starts_with("error[")
            || t.starts_with("Error:")
            || t.starts_with("Error during parsing")
    });
    if saw_diff && !saw_error_line {
        RustfmtFailureKind::FormattingDiff
    } else {
        RustfmtFailureKind::SyntaxFailure
    }
}

/// The rustup-shim "tool not installed" output shapes. All mean the
/// formatter never executed: no file was parsed, no diff produced.
fn rustfmt_output_is_tool_unavailable(combined: &str) -> bool {
    let lower = combined.to_lowercase();
    if !lower.contains("rustfmt") {
        return false;
    }
    lower.contains("does not have component")
        || (lower.contains("not installed") && lower.contains("toolchain"))
        || lower.contains("rustup component add rustfmt")
}

/// The "check not run" result for a rustfmt shim without the component:
/// advisory, non-blocking, and honest that nothing was checked. Follows the
/// sqlfluff-not-installed precedent in `run_cheap_syntax_check`.
fn rustfmt_unavailable_result(lang: RepoLanguage, duration_ms: u64, output: &str) -> CheckResult {
    CheckResult {
        not_run: true,
        check_type: CheckType::TypeCheck,
        passed: true,
        duration_ms,
        output: format!(
            "{lang} syntax check could not run: rustfmt is not installed for the active toolchain"
        ),
        errors: vec![],
        warnings: vec![format!(
            "{lang} syntax check skipped: rustfmt unavailable (rustup shim reports the \
             component is not installed); no files were checked"
        )],
        suggestions: vec![format!(
            "Install the component with `rustup component add rustfmt` (the check did not \
             run at all: {})",
            output.chars().take(120).collect::<String>()
        )],
    }
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
            working_dir: None,
        }
    }

    /// Set an explicit working directory for tool operations and path resolution.
    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Set an explicit working directory for tool operations and path resolution.
    pub fn set_working_dir(&mut self, dir: impl Into<PathBuf>) {
        self.working_dir = Some(dir.into());
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

    /// Resolve a file path, checking an explicit `working_dir` first for relative paths
    /// so that edits made in a subproject or nested directory are resolved correctly
    /// even if `project_root` points to an enclosing workspace.
    pub fn resolve_file_path(&self, file: &str) -> PathBuf {
        let p = Path::new(file);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            // Only an EXPLICIT working dir is consulted before the project
            // root. The implicit process-cwd fallback made every gate
            // without one resolve `Cargo.toml`/`src/main.rs` against
            // wherever the process happened to run (the selfware checkout
            // in tests, whose manifests then won over a Python project's —
            // CI-only `infer_repo_language` / side-effect failures). The
            // agent always sets the working dir explicitly (agent/mod.rs),
            // so the nested-subproject case keeps working.
            let Some(dir) = self.working_dir.as_ref() else {
                return self.project_root.join(p);
            };
            let wd_path = dir.join(p);
            if wd_path.exists() {
                return wd_path;
            }
            if self.project_root.join(p).exists() {
                self.project_root.join(p)
            } else {
                wd_path
            }
        }
    }

    /// Compute hash for a file's content
    fn compute_file_hash(&self, path: &str) -> Option<u64> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        use std::io::Read;

        let full_path = self.resolve_file_path(path);
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
                // Sanitized env + whole-process-group reap via run_reaped: the
                // post-edit command is project-controlled and may be arbitrary,
                // so it must neither inherit host credentials nor survive its
                // timeout to keep mutating files/holding locks downstream.
                let reaped =
                    run_reaped_args(&program, &parsed, &self.project_root, timeout_secs).await;
                match reaped {
                    Ok(out) if !out.timed_out => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let combined = if stderr.is_empty() {
                            stdout.to_string()
                        } else {
                            format!("{}\n{}", stdout, stderr)
                        };
                        (
                            out.success,
                            truncate_str(&combined, 4000),
                            check_start.elapsed().as_millis() as u64,
                        )
                    }
                    Ok(_) => (
                        false,
                        format!(
                            "Post-edit test command '{}' timed out after {} seconds",
                            cmd, timeout_secs
                        ),
                        timeout_duration.as_millis() as u64,
                    ),
                    Err(e) => (
                        false,
                        format!("Failed to run post-edit test command '{}': {}", cmd, e),
                        check_start.elapsed().as_millis() as u64,
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
                not_run: false,
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
            not_run: false,
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
        let stderr = String::from_utf8_lossy(&output.stderr);

        // A rustup shim without the rustfmt component exits non-zero with the
        // explanation on stderr — that is check-not-run (tool unavailable),
        // never a formatting failure, and the run summary must not print
        // "verification: failed" over it. Mirrors the classify_rustfmt_failure
        // ToolUnavailable arm (same blind spot, sibling site).
        if !output.success && rustfmt_output_is_tool_unavailable(&format!("{stdout}\n{stderr}")) {
            return Ok(CheckResult {
                not_run: true,
                check_type: CheckType::Format,
                passed: true,
                duration_ms: duration,
                output: "format check could not run: rustfmt is not installed for the active toolchain".to_string(),
                errors: vec![],
                warnings: vec![
                    "format check skipped: rustfmt unavailable (rustup shim reports the component is not installed); no files were checked".to_string(),
                ],
                suggestions: vec![
                    "Install the component with `rustup component add rustfmt` (the check did not run)".to_string(),
                ],
            });
        }

        Ok(CheckResult {
            not_run: false,
            check_type: CheckType::Format,
            passed: output.success,
            duration_ms: duration,
            output: if output.success {
                "Formatting check passed".to_string()
            } else {
                // stderr carries the diff summary on some rustfmt versions;
                // include it so a failure never renders as an empty box.
                format!("{}{}", stdout, stderr)
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

        let output = run_reaped(
            "cargo",
            &["test", "--no-fail-fast"],
            &self.project_root,
            timeout_secs,
        )
        .await?;

        if output.timed_out {
            // Timeout - the child was killed and reaped (see run_reaped);
            // return a graceful error as before.
            return Ok(CheckResult {
                not_run: false,
                check_type: CheckType::Test,
                passed: false,
                duration_ms: timeout_secs * 1000,
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

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse test failures from output
        let errors = parse_test_failures(&stdout, &stderr);

        Ok(CheckResult {
            not_run: false,
            check_type: CheckType::Test,
            passed: output.success,
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
            not_run: false,
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
            not_run: false,
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
            not_run: false,
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
            let path = self.resolve_file_path(file);
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
            if self.resolve_file_path(file).exists() {
                if *lang == RepoLanguage::JavaScript
                    && self.resolve_file_path("tsconfig.json").exists()
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
    ///
    /// Python, JavaScript, TypeScript, Java and C/C++ first resolve the
    /// project's language level (see [`super::syntax_toolchain`]): a direct
    /// tool invocation with the tool's built-in defaults rejected valid modern
    /// code as a syntax error. A verifier that is missing, cannot start, or
    /// cannot judge the project's language level yields a NOT-RUN result
    /// (`not_run`), never a pass or a failure (AGENTS.md Rule 3).
    async fn run_cheap_syntax_check(
        &self,
        lang: RepoLanguage,
        files: &[String],
    ) -> Result<CheckResult> {
        let start = Instant::now();
        let full_paths: Vec<_> = files.iter().map(|f| self.resolve_file_path(f)).collect();
        if full_paths.is_empty() {
            return Ok(CheckResult {
                not_run: false,
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 0,
                output: format!("No {} files to check", lang),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            });
        }
        // Rust only: one rustfmt invocation per resolved edition (a direct
        // rustfmt run does not read Cargo.toml and would otherwise parse as
        // Rust 2015, rejecting valid `async fn` as a syntax error).
        let mut rust_groups: Vec<(super::rust_edition::ResolvedEdition, Vec<PathBuf>)> = Vec::new();
        // Project-level build commands (`dotnet build`, `swift build`) must run
        // where the project file is, not in the edited file's directory (they
        // do not search upward and failed with "no project found").
        let mut project_level = false;
        // Scratch output for `csc` (removed on drop; never a fixed /tmp path).
        let mut _csc_out: Option<tempfile::TempDir> = None;

        let (program, args): (&str, Vec<String>) = match lang {
            RepoLanguage::Python => {
                return Ok(self.check_python_syntax(files, &full_paths, start).await)
            }
            RepoLanguage::JavaScript => {
                return Ok(self
                    .check_javascript_syntax(files, &full_paths, start)
                    .await)
            }
            RepoLanguage::TypeScript => {
                return Ok(self
                    .check_typescript_syntax(files, &full_paths, start)
                    .await)
            }
            RepoLanguage::Java => {
                return Ok(self.check_java_syntax(files, &full_paths, start).await)
            }
            RepoLanguage::Cpp => {
                return Ok(self.check_c_family_syntax(files, &full_paths, start).await)
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
                    project_level = true;
                    ("dotnet", vec!["build".to_string(), "--nologo".to_string()])
                } else {
                    let out = match tempfile::Builder::new()
                        .prefix("selfware-csharp-check")
                        .tempdir()
                    {
                        Ok(d) => d,
                        Err(e) => {
                            return Ok(syntax_not_run(
                                lang,
                                &format!("could not create a scratch output dir: {e}"),
                                0,
                            ))
                        }
                    };
                    let mut a = vec![
                        "-target:library".to_string(),
                        format!("-out:{}", out.path().join("check.dll").display()),
                    ];
                    for p in &full_paths {
                        a.push(p.to_string_lossy().to_string());
                    }
                    _csc_out = Some(out);
                    ("csc", a)
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
                        not_run: true,
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
                    project_level = true;
                    ("swift", vec!["build".to_string()])
                } else {
                    // Every edited file, not just the first.
                    let mut a = vec!["-parse".to_string()];
                    a.extend(full_paths.iter().map(|p| p.to_string_lossy().to_string()));
                    ("swiftc", a)
                }
            }
            RepoLanguage::Rust => {
                // Args are built per edition group below.
                rust_groups = super::rust_edition::group_by_edition(&full_paths);
                ("rustfmt", Vec::new())
            }
            RepoLanguage::Unknown => {
                return Ok(syntax_not_run(
                    lang,
                    "unknown language, no syntax checker applies",
                    0,
                ));
            }
        };

        if !self.command_exists(program).await {
            return Ok(syntax_not_run(
                lang,
                &format!("`{}` not found on PATH", program),
                0,
            ));
        }

        let check_dir = if project_level {
            self.project_root.clone()
        } else {
            full_paths
                .first()
                .and_then(|p| p.parent())
                .filter(|p| p.is_dir())
                .map(|p| p.to_path_buf())
                // Explicit working dir, then the project root -- never the
                // ambient process cwd (see `resolve_file_path`).
                .or_else(|| self.working_dir.clone())
                .unwrap_or_else(|| self.project_root.clone())
        };

        let invocations: Vec<Vec<String>> = if program == "rustfmt" {
            rust_groups
                .iter()
                .map(|(edition, paths)| {
                    let mut a = super::rust_edition::rustfmt_check_args(&edition.edition);
                    a.extend(paths.iter().map(|p| p.to_string_lossy().to_string()));
                    a
                })
                .collect()
        } else {
            vec![args]
        };

        let mut all_success = true;
        let mut combined = String::new();
        for args in &invocations {
            // Sanitized env + timeout + process-group reaping (run_reaped_env).
            match self.run_syntax_tool(program, args, &check_dir, &[]).await {
                ToolRun::NotRun(reason) => {
                    return Ok(syntax_not_run(
                        lang,
                        &reason,
                        start.elapsed().as_millis() as u64,
                    ));
                }
                ToolRun::Done { success, output } => {
                    all_success &= success;
                    if !output.is_empty() {
                        if !combined.is_empty() {
                            combined.push('\n');
                        }
                        combined.push_str(&output);
                    }
                }
            }
        }
        let duration = start.elapsed().as_millis() as u64;
        // Rule 3: name the edition each Rust file was parsed as, and flag a
        // guessed (fallback) edition, so a pass/fail says what was checked.
        let edition_notes: Vec<String> = rust_groups
            .iter()
            .map(|(edition, paths)| {
                format!("{} file(s) parsed as {}", paths.len(), edition.describe())
            })
            .collect();
        let edition_warnings: Vec<String> = rust_groups
            .iter()
            .filter(|(edition, _)| edition.is_fallback())
            .map(|(edition, _)| {
                format!(
                    "{} syntax check used a fallback edition: {}",
                    lang,
                    edition.describe()
                )
            })
            .collect();

        // rustfmt shares exit code 1 between parse failures and formatting
        // differences, so a non-zero `rustfmt --check` over valid-but-
        // unformatted code used to fail the SYNTAX gate and block the whole
        // verification run even though the code parses and every test passes
        // (finding C). Pure formatting diffs become an advisory note here;
        // parse failures and any unclassifiable failure still block below.
        // Rule 2: this changes one verification semantic on purpose — for
        // Rust, "syntax" means parseable, "format" means rustfmt-clean — and
        // formatting remains enforced by the separate cargo-fmt check when
        // `format_on_edit` is enabled. A real parse error NEVER goes green.
        //
        // A rustup shim without the rustfmt component is neither: the tool
        // never ran, so the result is check-not-run (advisory), never a
        // syntax failure (W7b finding 4).
        if !all_success && program == "rustfmt" {
            match classify_rustfmt_failure(&combined) {
                RustfmtFailureKind::ToolUnavailable => {
                    return Ok(rustfmt_unavailable_result(lang, duration, &combined));
                }
                RustfmtFailureKind::FormattingDiff => {
                    return Ok(CheckResult {
                        not_run: false,
                        check_type: CheckType::TypeCheck,
                        passed: true,
                        duration_ms: duration,
                        output: if edition_notes.is_empty() {
                            combined
                        } else {
                            format!("{}\n[{}]", combined, edition_notes.join("; "))
                        },
                        errors: vec![VerificationError {
                            file: files.first().cloned().unwrap_or_default(),
                            line: None,
                            column: None,
                            message: format!(
                                "{} formatting differs (rustfmt --check reported diffs, not a syntax error); run cargo fmt to apply",
                                lang
                            ),
                            code: Some("FORMATTING_DIFF".to_string()),
                            severity: ErrorSeverity::Note,
                            suggestion: Some(
                                "Run `cargo fmt` (or `rustfmt`) to apply the formatting"
                                    .to_string(),
                            ),
                        }],
                        warnings: std::iter::once(format!(
                            "{} syntax is valid; rustfmt --check only reports formatting differences ({})",
                            lang,
                            edition_notes.join("; ")
                        ))
                        .chain(edition_warnings)
                        .collect(),
                        suggestions: vec![
                            "Run `cargo fmt` to fix formatting before committing".to_string()
                        ],
                    });
                }
                RustfmtFailureKind::SyntaxFailure => {}
            }
        }

        Ok(CheckResult {
            not_run: false,
            check_type: CheckType::TypeCheck,
            passed: all_success,
            duration_ms: duration,
            output: if all_success {
                if edition_notes.is_empty() {
                    format!("{} syntax check passed", lang)
                } else {
                    format!(
                        "{} syntax check passed ({})",
                        lang,
                        edition_notes.join("; ")
                    )
                }
            } else if edition_notes.is_empty() {
                combined.clone()
            } else {
                format!("{}\n[{}]", combined, edition_notes.join("; "))
            },
            errors: if all_success {
                vec![]
            } else {
                let first_error = combined
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("syntax check failed");
                let first_error: String = first_error.chars().take(150).collect();
                vec![VerificationError {
                    file: files.first().cloned().unwrap_or_default(),
                    line: None,
                    column: None,
                    message: format!("{} syntax check failed: {}", lang, first_error),
                    code: None,
                    severity: ErrorSeverity::Error,
                    suggestion: Some(format!("Check {} syntax and fix errors", lang)),
                }]
            },
            warnings: edition_warnings,
            suggestions: if all_success {
                vec![]
            } else {
                vec![format!("Fix {} syntax errors before running tests", lang)]
            },
        })
    }

    /// Run one syntax-tool invocation: sanitized environment, the configured
    /// timeout, process-group reaping. A spawn failure is NOT-RUN (the tool
    /// never executed); a timeout stays a failure (fail-closed, like every
    /// other reaped verification command).
    async fn run_syntax_tool(
        &self,
        program: &str,
        args: &[String],
        cwd: &Path,
        env: &[(&str, &str)],
    ) -> ToolRun {
        match run_reaped_env(program, args, cwd, self.config.check_timeout_secs, env).await {
            Err(e) => ToolRun::NotRun(format!("`{}` could not be started: {:#}", program, e)),
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let output = if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    format!("{}\n{}", stdout, stderr)
                };
                ToolRun::Done {
                    success: out.success,
                    output,
                }
            }
        }
    }

    /// `node_modules/.bin/tsc` nearest `near`, else `tsc` on PATH. Never
    /// `npx`: it silently downloads a package when none is installed.
    async fn resolve_tsc(&self, near: &Path) -> Option<String> {
        if let Some(p) = super::syntax_toolchain::find_local_node_bin(near, "tsc") {
            return Some(p.to_string_lossy().to_string());
        }
        if self.command_exists("tsc").await {
            Some("tsc".to_string())
        } else {
            None
        }
    }

    /// Python: `compile()` (never writes bytecode) under the interpreter that
    /// matches the project's pinned version when one is installed. A host
    /// interpreter OLDER than the pin cannot judge newer syntax, so its
    /// rejection is reported as not-run, not as a syntax failure.
    async fn check_python_syntax(
        &self,
        files: &[String],
        paths: &[PathBuf],
        start: Instant,
    ) -> CheckResult {
        use super::syntax_toolchain as tc;
        let mut tally = SyntaxTally::new(RepoLanguage::Python, files);
        let mut groups: Vec<(Option<tc::PythonPin>, Vec<PathBuf>)> = Vec::new();
        for p in paths {
            let pin = tc::resolve_python_pin(p, &self.project_root);
            match groups.iter_mut().find(|(g, _)| *g == pin) {
                Some((_, v)) => v.push(p.clone()),
                None => groups.push((pin, vec![p.clone()])),
            }
        }
        for (pin, group) in groups {
            let mut interp = "python3".to_string();
            if let Some(pin) = &pin {
                let exact = format!("python{}.{}", pin.min.0, pin.min.1);
                if self.command_exists(&exact).await {
                    interp = exact;
                }
            }
            if !self.command_exists(&interp).await {
                tally.record(ToolRun::NotRun(format!("`{}` not found on PATH", interp)));
                continue;
            }
            let mut args = vec![
                "-B".to_string(),
                "-c".to_string(),
                tc::PYTHON_CHECK_SCRIPT.to_string(),
            ];
            args.extend(group.iter().map(|p| p.to_string_lossy().to_string()));
            let cwd = dir_of(&group[0]).unwrap_or_else(|| self.project_root.clone());
            let run = self
                .run_syntax_tool(&interp, &args, &cwd, &[("PYTHONDONTWRITEBYTECODE", "1")])
                .await;
            let ToolRun::Done { success, output } = run else {
                tally.record(run);
                continue;
            };
            let host = tc::parse_python_check_version(&output);
            let output = output
                .lines()
                .filter(|l| !l.starts_with("selfware-python-version "))
                .collect::<Vec<_>>()
                .join("\n");
            let host_desc = host
                .map(|(a, b)| format!("{a}.{b}"))
                .unwrap_or_else(|| "(unknown version)".to_string());
            let pin_desc = pin
                .as_ref()
                .map(|p| format!("; project requires {}.{} ({})", p.min.0, p.min.1, p.source))
                .unwrap_or_default();
            tally.notes.push(format!(
                "{} file(s) compiled by {} {}{}",
                group.len(),
                interp,
                host_desc,
                pin_desc
            ));
            if host.is_none() && !success {
                tally.record(ToolRun::NotRun(format!(
                    "{} did not run the check script: {}",
                    interp,
                    first_nonempty_line(&output)
                )));
                continue;
            }
            if let (Some(pin), Some(h)) = (&pin, host) {
                if h < pin.min {
                    if !success {
                        tally.record(ToolRun::NotRun(format!(
                            "host {} is {}, older than the project's Python {}.{} ({}); it cannot judge newer syntax, so its rejection is not a verdict (install python{}.{} to verify)",
                            interp, host_desc, pin.min.0, pin.min.1, pin.source, pin.min.0, pin.min.1
                        )));
                        continue;
                    }
                    tally.warnings.push(format!(
                        "Python syntax checked with {} {}, older than the project's Python {}.{} ({})",
                        interp, host_desc, pin.min.0, pin.min.1, pin.source
                    ));
                }
            }
            tally.record(ToolRun::Done { success, output });
        }
        tally.finish(start.elapsed().as_millis() as u64)
    }

    /// JavaScript: `node --check` on EVERY file (it only ever checked the
    /// first). ES-module syntax that node would parse as CommonJS is checked
    /// as a module (an `.mjs` copy, which every node version parses as ESM);
    /// JSX, which node cannot parse at all, goes to `tsc --allowJs`
    /// (syntactic diagnostics only).
    async fn check_javascript_syntax(
        &self,
        files: &[String],
        paths: &[PathBuf],
        start: Instant,
    ) -> CheckResult {
        use super::syntax_toolchain::{self as tc, JsMode};
        let mut tally = SyntaxTally::new(RepoLanguage::JavaScript, files);
        let node_ok = self.command_exists("node").await;
        for p in paths {
            let label = file_label(p);
            let src = std::fs::read_to_string(p).unwrap_or_default();
            let cwd = dir_of(p).unwrap_or_else(|| self.project_root.clone());
            let mode = tc::js_mode(p, &self.project_root, &src);
            if mode == JsMode::Jsx {
                tally.notes.push(format!("{label}: JSX, parsed by tsc"));
                let run = self.jsx_syntax_run(p, &cwd).await;
                tally.record(run);
                continue;
            }
            if !node_ok {
                tally.record(ToolRun::NotRun("`node` not found on PATH".to_string()));
                continue;
            }
            let path_str = p.to_string_lossy().to_string();
            let run = match &mode {
                JsMode::ForceModule(why) => {
                    tally
                        .notes
                        .push(format!("{label}: parsed as an ES module ({why})"));
                    self.node_check_as_module(p, &cwd).await
                }
                JsMode::Native(why) => {
                    tally.notes.push(format!("{label}: {why}"));
                    self.run_syntax_tool("node", &["--check".to_string(), path_str], &cwd, &[])
                        .await
                }
                JsMode::Jsx => unreachable!("handled above"),
            };
            // JSX inside a `.js` file (React without a .jsx extension): node
            // rejects the `<`; let a JSX-aware parser decide instead.
            if let ToolRun::Done {
                success: false,
                output,
            } = &run
            {
                if tc::node_output_suggests_jsx(output) {
                    tally.notes.push(format!(
                        "{label}: node rejected `<`, re-parsed as JSX by tsc"
                    ));
                    let jsx = self.jsx_syntax_run(p, &cwd).await;
                    tally.record(jsx);
                    continue;
                }
            }
            tally.record(run);
        }
        tally.finish(start.elapsed().as_millis() as u64)
    }

    /// `node --check` on a temporary `.mjs` copy of `p` (parse goal: module),
    /// with the temp path rewritten back to `p` in the output.
    async fn node_check_as_module(&self, p: &Path, cwd: &Path) -> ToolRun {
        let tmp = match tempfile::Builder::new()
            .prefix("selfware-js-check")
            .tempdir()
        {
            Ok(t) => t,
            Err(e) => return ToolRun::NotRun(format!("could not create a temp dir: {e}")),
        };
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "module".to_string());
        let copy = tmp.path().join(format!("{stem}.mjs"));
        if let Err(e) = std::fs::copy(p, &copy) {
            return ToolRun::NotRun(format!("could not stage {} for checking: {e}", p.display()));
        }
        let run = self
            .run_syntax_tool(
                "node",
                &["--check".to_string(), copy.to_string_lossy().to_string()],
                cwd,
                &[],
            )
            .await;
        match run {
            ToolRun::Done { success, output } => ToolRun::Done {
                success,
                output: output.replace(
                    copy.to_string_lossy().as_ref(),
                    p.to_string_lossy().as_ref(),
                ),
            },
            other => other,
        }
    }

    /// Syntax-only JSX parse via `tsc --allowJs` (no `checkJs`: JavaScript
    /// files get syntactic diagnostics only).
    async fn jsx_syntax_run(&self, p: &Path, cwd: &Path) -> ToolRun {
        let Some(tsc) = self.resolve_tsc(p).await else {
            return ToolRun::NotRun(format!(
                "{} contains JSX, which node cannot parse, and no TypeScript compiler (`tsc`) is installed to parse it (npx is not used: it would download packages)",
                file_label(p)
            ));
        };
        let mut args = super::syntax_toolchain::jsx_syntax_args();
        args.push(p.to_string_lossy().to_string());
        self.run_syntax_tool(&tsc, &args, cwd, &[]).await
    }

    /// TypeScript: with a `tsconfig.json`, check through a temporary config
    /// that `extends` it and lists only the edited files (project options,
    /// no whole-project typecheck); without one, explicit modern defaults
    /// (reported as a fallback). The compiler is the project-local
    /// `node_modules/.bin/tsc` or `tsc` on PATH — never `npx`.
    async fn check_typescript_syntax(
        &self,
        files: &[String],
        paths: &[PathBuf],
        start: Instant,
    ) -> CheckResult {
        use super::syntax_toolchain as tc;
        static WRAPPER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut tally = SyntaxTally::new(RepoLanguage::TypeScript, files);
        let mut groups: Vec<(Option<PathBuf>, Vec<PathBuf>)> = Vec::new();
        for p in paths {
            let cfg = tc::resolve_ts_project(p, &self.project_root);
            match groups.iter_mut().find(|(g, _)| *g == cfg) {
                Some((_, v)) => v.push(p.clone()),
                None => groups.push((cfg, vec![p.clone()])),
            }
        }
        for (cfg, group) in groups {
            let near = cfg.clone().unwrap_or_else(|| group[0].clone());
            let Some(tsc) = self.resolve_tsc(&near).await else {
                tally.record(ToolRun::NotRun(
                    "TypeScript compiler not installed: no node_modules/.bin/tsc above the file and no `tsc` on PATH (npx is not used: it would silently download packages)".to_string(),
                ));
                continue;
            };
            if let Some(cfg) = &cfg {
                let dir = cfg.parent().map(Path::to_path_buf).unwrap_or_default();
                let wrapper = dir.join(format!(
                    ".selfware-syntax-check-{}-{}.json",
                    std::process::id(),
                    WRAPPER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                match std::fs::write(&wrapper, tc::ts_wrapper_config_json(cfg, &group)) {
                    Ok(()) => {
                        let _cleanup = RemoveOnDrop(wrapper.clone());
                        tally.notes.push(format!(
                            "{} file(s) checked with project config {}",
                            group.len(),
                            cfg.display()
                        ));
                        let args = vec!["-p".to_string(), wrapper.to_string_lossy().to_string()];
                        let run = self.run_syntax_tool(&tsc, &args, &dir, &[]).await;
                        tally.record(run);
                        continue;
                    }
                    Err(e) => tally.warnings.push(format!(
                        "could not write a temporary tsconfig next to {} ({e}); checked with fallback compiler options instead",
                        cfg.display()
                    )),
                }
            }
            let mut args = tc::ts_fallback_args();
            args.extend(group.iter().map(|p| p.to_string_lossy().to_string()));
            tally.notes.push(format!(
                "{} file(s) checked with fallback options (no tsconfig.json)",
                group.len()
            ));
            tally.warnings.push(format!(
                "TypeScript syntax check used fallback compiler options ({}): no tsconfig.json applies to {}",
                tc::ts_fallback_args().join(" "),
                group.iter().map(|p| file_label(p)).collect::<Vec<_>>().join(", ")
            ));
            let cwd = dir_of(&group[0]).unwrap_or_else(|| self.project_root.clone());
            let run = self.run_syntax_tool(&tsc, &args, &cwd, &[]).await;
            tally.record(run);
        }
        tally.finish(start.elapsed().as_millis() as u64)
    }

    /// Java: `--release` from the build file when the host javac supports
    /// it, `-sourcepath` from the package declaration, class output into a
    /// temp dir that is removed afterwards (it used to be `/tmp` itself).
    async fn check_java_syntax(
        &self,
        files: &[String],
        paths: &[PathBuf],
        start: Instant,
    ) -> CheckResult {
        use super::syntax_toolchain as tc;
        let mut tally = SyntaxTally::new(RepoLanguage::Java, files);
        if !self.command_exists("javac").await {
            tally.record(ToolRun::NotRun("`javac` not found on PATH".to_string()));
            return tally.finish(start.elapsed().as_millis() as u64);
        }
        let host = match self
            .run_syntax_tool("javac", &["-version".to_string()], &self.project_root, &[])
            .await
        {
            ToolRun::Done { output, .. } => tc::parse_javac_version(&output),
            ToolRun::NotRun(r) => {
                tally.record(ToolRun::NotRun(r));
                return tally.finish(start.elapsed().as_millis() as u64);
            }
        };
        let mut groups: Vec<(Option<tc::JavaRelease>, Vec<PathBuf>)> = Vec::new();
        for p in paths {
            let rel = tc::resolve_java_release(p, &self.project_root);
            match groups.iter_mut().find(|(g, _)| *g == rel) {
                Some((_, v)) => v.push(p.clone()),
                None => groups.push((rel, vec![p.clone()])),
            }
        }
        for (rel, group) in groups {
            let out = match tempfile::Builder::new().prefix("selfware-javac-").tempdir() {
                Ok(d) => d,
                Err(e) => {
                    tally.record(ToolRun::NotRun(format!(
                        "could not create a class output dir: {e}"
                    )));
                    continue;
                }
            };
            let mut base = vec![
                "-Xlint:none".to_string(),
                "-proc:none".to_string(),
                "-implicit:none".to_string(),
                "-d".to_string(),
                out.path().to_string_lossy().to_string(),
            ];
            let mut roots: Vec<PathBuf> = Vec::new();
            for p in &group {
                let src = std::fs::read_to_string(p).unwrap_or_default();
                if let Some(r) = tc::java_source_root(p, &src) {
                    if !roots.contains(&r) {
                        roots.push(r);
                    }
                }
            }
            if let Ok(sp) = std::env::join_paths(&roots) {
                if !roots.is_empty() {
                    base.push("-sourcepath".to_string());
                    base.push(sp.to_string_lossy().to_string());
                }
            }
            let host_desc = host
                .map(|h| h.to_string())
                .unwrap_or_else(|| "(unknown version)".to_string());
            let mut release_args: Vec<String> = Vec::new();
            let mut host_too_old = false;
            match (&rel, host) {
                (Some(r), Some(h)) if r.release > h => {
                    host_too_old = true;
                    tally.warnings.push(format!(
                        "host javac {} is older than the project's Java {} ({})",
                        h, r.release, r.source
                    ));
                }
                (Some(r), Some(h)) if h >= 9 && r.release >= 8 => {
                    release_args = vec!["--release".to_string(), r.release.to_string()];
                    tally.notes.push(format!(
                        "{} file(s) compiled with javac {} --release {} ({})",
                        group.len(),
                        h,
                        r.release,
                        r.source
                    ));
                }
                (Some(r), _) => tally.warnings.push(format!(
                    "project Java {} ({}) not passed as --release to javac {}",
                    r.release, r.source, host_desc
                )),
                (None, _) => tally.warnings.push(format!(
                    "no Java release level found in pom.xml/build.gradle; javac {} used its default",
                    host_desc
                )),
            }
            let file_args: Vec<String> = group
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            let cwd = dir_of(&group[0]).unwrap_or_else(|| self.project_root.clone());
            let mut args = base.clone();
            args.extend(release_args.iter().cloned());
            args.extend(file_args.iter().cloned());
            let mut run = self.run_syntax_tool("javac", &args, &cwd, &[]).await;
            if let ToolRun::Done {
                success: false,
                output,
            } = &run
            {
                if !release_args.is_empty()
                    && output.contains("release version")
                    && output.contains("not supported")
                {
                    tally.warnings.push(format!(
                        "javac {} does not support --release {}; re-checked with its default",
                        host_desc, release_args[1]
                    ));
                    let mut args = base.clone();
                    args.extend(file_args.iter().cloned());
                    run = self.run_syntax_tool("javac", &args, &cwd, &[]).await;
                }
            }
            if host_too_old {
                if let (ToolRun::Done { success: false, .. }, Some(r)) = (&run, &rel) {
                    tally.record(ToolRun::NotRun(format!(
                        "host javac {} is older than the project's Java {} ({}); it cannot judge newer syntax, so its rejection is not a verdict",
                        host_desc, r.release, r.source
                    )));
                    continue;
                }
            }
            tally.record(run);
            drop(out);
        }
        tally.finish(start.elapsed().as_millis() as u64)
    }

    /// C/C++: each file parsed with `-fsyntax-only` under the standard (and
    /// include paths/defines) the project actually builds with:
    /// `compile_commands.json` → CMake standard → modern fallback. This
    /// replaces `cmake --build .` for CMake projects, which ran in the edited
    /// file's directory (never a configured build tree) and so failed on
    /// every edit, and it checks every file instead of only the first.
    async fn check_c_family_syntax(
        &self,
        files: &[String],
        paths: &[PathBuf],
        start: Instant,
    ) -> CheckResult {
        use super::syntax_toolchain as tc;
        let mut tally = SyntaxTally::new(RepoLanguage::Cpp, files);
        for p in paths {
            let resolved = tc::resolve_c_flags(p, &self.project_root);
            let compiler = resolved.lang.compiler();
            if !self.command_exists(compiler).await {
                tally.record(ToolRun::NotRun(format!("`{}` not found on PATH", compiler)));
                continue;
            }
            let std = resolved
                .flags
                .iter()
                .find(|f| f.starts_with("-std="))
                .cloned()
                .unwrap_or_default();
            tally.notes.push(format!(
                "{}: {} {} ({})",
                file_label(p),
                compiler,
                std,
                resolved.source
            ));
            if resolved.fallback {
                tally.warnings.push(format!(
                    "C/C++ syntax check of {} used a fallback standard: {}",
                    file_label(p),
                    resolved.source
                ));
            }
            let mut args = resolved.flags.clone();
            args.extend([
                "-fsyntax-only".to_string(),
                "-x".to_string(),
                resolved.lang.x_lang().to_string(),
                p.to_string_lossy().to_string(),
            ]);
            let cwd = dir_of(p).unwrap_or_else(|| self.project_root.clone());
            let run = self.run_syntax_tool(compiler, &args, &cwd, &[]).await;
            tally.record(run);
        }
        tally.finish(start.elapsed().as_millis() as u64)
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
                not_run: false,
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

        let output = run_reaped_args(&program, &args, &self.project_root, timeout_secs).await?;

        if output.timed_out {
            return Ok(CheckResult {
                not_run: false,
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

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(CheckResult {
            not_run: false,
            check_type: CheckType::Test,
            passed: output.success,
            duration_ms: duration,
            output: format!("{}\n{}", stdout, stderr),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        })
    }

    async fn command_exists(&self, cmd: &str) -> bool {
        let mut which = Command::new("which");
        crate::safety::process_env::sanitize_command_env(&mut which);
        match which.arg(cmd).output().await {
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

/// Outcome of one syntax-tool invocation.
enum ToolRun {
    /// The tool ran to completion (or timed out: fail-closed).
    Done { success: bool, output: String },
    /// The tool never ran / cannot judge; the string says why.
    NotRun(String),
}

/// The NOT-RUN result: non-blocking (`passed`), flagged `not_run`, no
/// errors, and explicit that nothing was verified (AGENTS.md Rule 3).
fn syntax_not_run(lang: RepoLanguage, reason: &str, duration_ms: u64) -> CheckResult {
    CheckResult {
        check_type: CheckType::TypeCheck,
        passed: true,
        not_run: true,
        duration_ms,
        output: format!("{} syntax check could not run: {}", lang, reason),
        errors: vec![],
        warnings: vec![format!(
            "{} syntax check NOT RUN (no files were verified): {}",
            lang, reason
        )],
        suggestions: vec![format!(
            "Install the {} verifier or run a project-specific check manually",
            lang
        )],
    }
}

/// Accumulates per-file / per-group syntax runs into one [`CheckResult`]:
/// any failure fails; nothing completed → not-run; otherwise a pass that
/// names what was checked and lists any part that did not run.
struct SyntaxTally {
    lang: RepoLanguage,
    first_file: String,
    completed: usize,
    failures: Vec<String>,
    not_run: Vec<String>,
    /// What was checked and how (resolution provenance).
    notes: Vec<String>,
    /// Fallbacks and caveats.
    warnings: Vec<String>,
}

impl SyntaxTally {
    fn new(lang: RepoLanguage, files: &[String]) -> Self {
        Self {
            lang,
            first_file: files.first().cloned().unwrap_or_default(),
            completed: 0,
            failures: Vec::new(),
            not_run: Vec::new(),
            notes: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn record(&mut self, run: ToolRun) {
        match run {
            ToolRun::Done { success, output } => {
                self.completed += 1;
                if !success {
                    self.failures.push(if output.trim().is_empty() {
                        "syntax check failed (no output)".to_string()
                    } else {
                        output
                    });
                }
            }
            ToolRun::NotRun(reason) => self.not_run.push(reason),
        }
    }

    fn finish(self, duration_ms: u64) -> CheckResult {
        let lang = self.lang;
        let notes = if self.notes.is_empty() {
            String::new()
        } else {
            format!(" [{}]", self.notes.join("; "))
        };
        if self.completed == 0 && self.failures.is_empty() {
            let reason = if self.not_run.is_empty() {
                "no verifier ran".to_string()
            } else {
                self.not_run.join("; ")
            };
            let mut r = syntax_not_run(lang, &reason, duration_ms);
            r.warnings.extend(self.warnings);
            return r;
        }
        let mut warnings = self.warnings;
        warnings.extend(
            self.not_run
                .iter()
                .map(|r| format!("{} syntax check NOT RUN for part of the edit: {}", lang, r)),
        );
        if !self.failures.is_empty() {
            let combined = self.failures.join("\n");
            let first: String = first_nonempty_line(&combined).chars().take(150).collect();
            return CheckResult {
                check_type: CheckType::TypeCheck,
                passed: false,
                not_run: false,
                duration_ms,
                output: format!("{}{}", combined, notes),
                errors: vec![VerificationError {
                    file: self.first_file,
                    line: None,
                    column: None,
                    message: format!("{} syntax check failed: {}", lang, first),
                    code: None,
                    severity: ErrorSeverity::Error,
                    suggestion: Some(format!("Check {} syntax and fix errors", lang)),
                }],
                warnings,
                suggestions: vec![format!("Fix {} syntax errors before running tests", lang)],
            };
        }
        let mut output = format!("{} syntax check passed{}", lang, notes);
        if !self.not_run.is_empty() {
            output.push_str(&format!(
                "; NOT RUN for {} part(s): {}",
                self.not_run.len(),
                self.not_run.join("; ")
            ));
        }
        CheckResult {
            check_type: CheckType::TypeCheck,
            passed: true,
            not_run: false,
            duration_ms,
            output,
            errors: vec![],
            warnings,
            suggestions: vec![],
        }
    }
}

fn first_nonempty_line(s: &str) -> &str {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("syntax check failed")
}

fn file_label(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

fn dir_of(p: &Path) -> Option<PathBuf> {
    p.parent().filter(|d| d.is_dir()).map(Path::to_path_buf)
}

/// Removes a temporary file (the TypeScript wrapper config) when dropped,
/// including on early return or panic.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
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
            // A check that did not run is neither a pass nor a failure.
            let status = if check.not_run {
                "○"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            writeln!(
                f,
                "║ {} {}: {}ms{}",
                status,
                check.check_type.as_str(),
                check.duration_ms,
                if check.not_run { " (not run)" } else { "" }
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
