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

/// Quick-and-dirty verification gate for any Rust file edit.
/// This is the "Day 1" implementation — wrap existing file_edit/file_write
/// tools with a cargo check gate.
pub fn verify_edit_or_rollback(repo_root: &Path, edited_file: &Path) -> Result<bool, String> {
    // Only gate Rust files
    if edited_file.extension().and_then(|e| e.to_str()) != Some("rs") {
        return Ok(true);
    }

    let diagnostics = cargo_check_json(repo_root)?;
    let has_errors = diagnostics
        .iter()
        .any(|d| d.level == DiagnosticLevel::Error);

    if has_errors {
        // Rollback the edit
        let _ = Command::new("git")
            .args(["checkout", "--"])
            .arg(edited_file)
            .current_dir(repo_root)
            .output();
    }

    Ok(!has_errors)
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
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_parsing() {
        let json = serde_json::json!({
            "level": "error",
            "message": "cannot find value `x` in this scope",
            "spans": [{
                "is_primary": true,
                "file_name": "src/main.rs",
                "line_start": 42,
                "column_start": 5,
                "text": [{ "text": "    let y = x + 1;" }]
            }]
        });

        let diag = parse_diagnostic(&json).unwrap();
        assert_eq!(diag.level, DiagnosticLevel::Error);
        assert_eq!(diag.line, 42);
        assert_eq!(diag.file, "src/main.rs");
    }

    #[test]
    fn test_error_prompt_formatting() {
        let result = AstMutationResult::compile_failed(vec![CompilerDiagnostic {
            level: DiagnosticLevel::Error,
            message: "mismatched types".to_string(),
            file: "src/memory.rs".to_string(),
            line: 301,
            column: 12,
            span_text: "fn evict_oldest(&mut self) -> u64".to_string(),
        }]);

        let prompt = result.error_prompt();
        assert!(prompt.contains("FROST"));
        assert!(prompt.contains("mismatched types"));
        assert!(prompt.contains("memory.rs:301"));
    }

    #[test]
    fn test_not_found_result() {
        let result = AstMutationResult::not_found("nonexistent_fn");
        assert!(!result.success);
        assert!(result.error_prompt().contains("nonexistent_fn"));
    }

    #[test]
    fn test_is_protected_from_parent() {
        use super::super::is_protected;
        assert!(is_protected(Path::new("src/evolution/ast_tools.rs")));
        assert!(!is_protected(Path::new("src/tools/file_edit.rs")));
    }

    #[test]
    fn test_error_prompt_success_case() {
        let result = AstMutationResult {
            success: true,
            compiler_errors: vec![],
            diff: "some diff".to_string(),
            worktree_path: Some(PathBuf::from("/tmp/test")),
        };
        assert_eq!(result.error_prompt(), "Mutation compiled successfully.");
    }

    #[test]
    fn test_compile_failed_empty_errors() {
        let result = AstMutationResult::compile_failed(vec![]);
        assert!(!result.success);
        assert!(result.compiler_errors.is_empty());
        assert!(result.diff.is_empty());
        assert!(result.worktree_path.is_none());
        // error_prompt should still show FROST header even with no errors
        let prompt = result.error_prompt();
        assert!(prompt.contains("FROST"));
    }

    #[test]
    fn test_diagnostic_parsing_missing_primary() {
        let json = serde_json::json!({
            "level": "error",
            "message": "some error",
            "spans": [{
                "is_primary": false,
                "file_name": "src/main.rs",
                "line_start": 1,
                "column_start": 1,
                "text": [{ "text": "code" }]
            }]
        });
        // No primary span → parse_diagnostic returns None
        assert!(parse_diagnostic(&json).is_none());
    }

    #[test]
    fn test_diagnostic_parsing_unknown_level() {
        let json = serde_json::json!({
            "level": "ice",
            "message": "internal compiler error",
            "spans": [{
                "is_primary": true,
                "file_name": "src/main.rs",
                "line_start": 1,
                "column_start": 1,
                "text": [{ "text": "code" }]
            }]
        });
        assert!(parse_diagnostic(&json).is_none());
    }

    #[test]
    fn test_diagnostic_parsing_missing_fields() {
        // Completely empty JSON
        let json = serde_json::json!({});
        assert!(parse_diagnostic(&json).is_none());

        // Has level but no message
        let json = serde_json::json!({
            "level": "error"
        });
        assert!(parse_diagnostic(&json).is_none());

        // Has level and message but no spans
        let json = serde_json::json!({
            "level": "error",
            "message": "test"
        });
        assert!(parse_diagnostic(&json).is_none());
    }

    #[test]
    fn test_uuid_short_uniqueness() {
        let a = uuid_short();
        // Small sleep to ensure different nanos
        std::thread::sleep(std::time::Duration::from_millis(1));
        let b = uuid_short();
        assert_ne!(a, b, "Two uuid_short calls should produce different values");
    }

    #[test]
    fn test_error_prompt_multiple_errors() {
        let result = AstMutationResult::compile_failed(vec![
            CompilerDiagnostic {
                level: DiagnosticLevel::Error,
                message: "type mismatch".to_string(),
                file: "src/lib.rs".to_string(),
                line: 10,
                column: 5,
                span_text: "let x: u32 = \"hello\"".to_string(),
            },
            CompilerDiagnostic {
                level: DiagnosticLevel::Warning,
                message: "unused variable".to_string(),
                file: "src/lib.rs".to_string(),
                line: 20,
                column: 9,
                span_text: String::new(), // empty span
            },
        ]);
        let prompt = result.error_prompt();
        assert!(prompt.contains("type mismatch"));
        assert!(prompt.contains("unused variable"));
        assert!(prompt.contains("lib.rs:10"));
        assert!(prompt.contains("lib.rs:20"));
        // span_text present only for first error
        assert!(prompt.contains("let x: u32"));
    }

    #[test]
    fn test_diagnostic_all_levels() {
        for (level_str, expected) in [
            ("error", DiagnosticLevel::Error),
            ("warning", DiagnosticLevel::Warning),
            ("note", DiagnosticLevel::Note),
            ("help", DiagnosticLevel::Help),
        ] {
            let json = serde_json::json!({
                "level": level_str,
                "message": "test message",
                "spans": [{
                    "is_primary": true,
                    "file_name": "test.rs",
                    "line_start": 1,
                    "column_start": 1,
                    "text": [{ "text": "code" }]
                }]
            });
            let diag = parse_diagnostic(&json).unwrap();
            assert_eq!(diag.level, expected);
        }
    }

    #[test]
    fn test_worktree_error_display() {
        let git_err = WorktreeError::GitFailed("branch conflict".to_string());
        assert!(format!("{}", git_err).contains("branch conflict"));

        let io_err = WorktreeError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(format!("{}", io_err).contains("IO error"));
    }
}
