//! AST-Aware Mutation Tools
//!
//! Uses the `syn` crate to manipulate Rust code at the AST level rather than
//! as raw strings. Every mutation is gated by a synchronous `cargo check` —
//! the compiler acts as the "laws of physics" that prune invalid mutations
//! before they waste context windows or evaluation cycles.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A request to mutate a specific function via AST transformation
#[derive(Debug, Clone)]
pub struct AstMutationRequest {
    /// Path to the target file (relative to repo root)
    pub target_file: PathBuf,
    /// Name of the function to mutate
    pub target_fn: String,
    /// Type of mutation to apply
    pub mutation_type: MutationType,
    /// For ReplaceFnBody: the new function body as Rust code
    pub new_body: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MutationType {
    /// Replace the entire function body
    ReplaceFnBody,
    /// Add a parameter to the function signature
    AddParameter { name: String, ty: String },
    /// Wrap the function's core logic in a cache layer
    WrapInCache { cache_key: String },
    /// Extract the function into its own module
    ExtractToModule { module_name: String },
    /// Inline all constant expressions
    InlineConstants,
}

/// Result of an AST mutation attempt
#[derive(Debug)]
pub struct AstMutationResult {
    /// Whether the mutation compiled successfully
    pub success: bool,
    /// Compiler errors (empty if success)
    pub compiler_errors: Vec<CompilerDiagnostic>,
    /// Unified diff of the change
    pub diff: String,
    /// Path to the git worktree containing the mutation
    pub worktree_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CompilerDiagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub span_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
}

impl AstMutationResult {
    pub fn compile_failed(errors: Vec<CompilerDiagnostic>) -> Self {
        Self {
            success: false,
            compiler_errors: errors,
            diff: String::new(),
            worktree_path: None,
        }
    }

    pub fn not_found(fn_name: &str) -> Self {
        Self {
            success: false,
            compiler_errors: vec![CompilerDiagnostic {
                level: DiagnosticLevel::Error,
                message: format!("Function `{}` not found in target file", fn_name),
                file: String::new(),
                line: 0,
                column: 0,
                span_text: String::new(),
            }],
            diff: String::new(),
            worktree_path: None,
        }
    }

    /// Format errors for injection into agent's working memory
    pub fn error_prompt(&self) -> String {
        if self.success {
            return String::from("Mutation compiled successfully.");
        }
        let mut prompt = String::from("FROST ❄️ — Compiler rejected mutation:\n\n");
        for err in &self.compiler_errors {
            prompt.push_str(&format!(
                "  [{:?}] {}:{},{}: {}\n",
                err.level, err.file, err.line, err.column, err.message
            ));
            if !err.span_text.is_empty() {
                prompt.push_str(&format!("         | {}\n", err.span_text));
            }
        }
        prompt
    }
}

/// Create an isolated git worktree for mutation testing
pub fn create_shadow_worktree(repo_root: &Path) -> Result<PathBuf, WorktreeError> {
    let worktree_name = format!("evolution-{}", uuid_short());
    let worktree_path = repo_root.join(".worktrees").join(&worktree_name);

    let output = Command::new("git")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg("HEAD")
        .current_dir(repo_root)
        .output()
        .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(WorktreeError::GitFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(worktree_path)
}

/// Remove a git worktree after evaluation
pub fn cleanup_worktree(repo_root: &Path, worktree_path: &Path) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_root)
        .output()
        .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;

    if !output.status.success() {
        // Force cleanup if normal removal fails
        let _ = std::fs::remove_dir_all(worktree_path);
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();
    }

    Ok(())
}

/// Run `cargo check` and parse JSON diagnostics
pub fn cargo_check_json(working_dir: &Path) -> Result<Vec<CompilerDiagnostic>, String> {
    let output = Command::new("cargo")
        .args(["check", "--message-format=json"])
        .current_dir(working_dir)
        .output()
        .map_err(|e| format!("Failed to run cargo check: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut diagnostics = Vec::new();

    for line in stdout.lines() {
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
            if msg["reason"] == "compiler-message" {
                if let Some(diag) = parse_diagnostic(&msg["message"]) {
                    diagnostics.push(diag);
                }
            }
        }
    }

    Ok(diagnostics)
}

fn parse_diagnostic(msg: &serde_json::Value) -> Option<CompilerDiagnostic> {
    let level = match msg["level"].as_str()? {
        "error" => DiagnosticLevel::Error,
        "warning" => DiagnosticLevel::Warning,
        "note" => DiagnosticLevel::Note,
        "help" => DiagnosticLevel::Help,
        _ => return None,
    };

    let message = msg["message"].as_str()?.to_string();

    // Extract primary span
    let spans = msg["spans"].as_array()?;
    let primary = spans
        .iter()
        .find(|s| s["is_primary"].as_bool() == Some(true))?;

    Some(CompilerDiagnostic {
        level,
        message,
        file: primary["file_name"].as_str().unwrap_or("").to_string(),
        line: primary["line_start"].as_u64().unwrap_or(0) as u32,
        column: primary["column_start"].as_u64().unwrap_or(0) as u32,
        span_text: primary["text"]
            .as_array()
            .and_then(|t| t.first())
            .and_then(|t| t["text"].as_str())
            .unwrap_or("")
            .to_string(),
    })
}

fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", t % 0xFFFF_FFFF)
}

#[derive(Debug)]
pub enum WorktreeError {
    GitFailed(String),
    IoError(std::io::Error),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitFailed(msg) => write!(f, "Git worktree operation failed: {}", msg),
            Self::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for WorktreeError {}

#[cfg(test)]
#[path = "../../tests/unit/evolution/ast_tools/ast_tools_test.rs"]
mod tests;
