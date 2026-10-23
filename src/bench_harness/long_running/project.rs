//! Project types and scaffolding for long-running tests.

use crate::safety::git_exec::{GitScope, SanitizedGitExt};
use std::path::Path;
use std::process::Command;

/// The type of project to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    /// Rust project (Cargo).
    Rust,
    /// Python project.
    Python,
    /// Go project.
    Go,
    /// Template-based project.
    Template,
}

impl ProjectType {
    /// Detect the project type from a directory.
    pub fn detect(dir: &Path) -> Option<Self> {
        if dir.join("Cargo.toml").exists() {
            Some(Self::Rust)
        } else if dir.join("go.mod").exists() {
            Some(Self::Go)
        } else if dir.join("setup.py").exists()
            || dir.join("pyproject.toml").exists()
            || dir.read_dir().ok()?.any(|e| {
                e.ok()
                    .map(|e| e.path().extension().map(|e| e == "py").unwrap_or(false))
                    .unwrap_or(false)
            })
        {
            Some(Self::Python)
        } else {
            None
        }
    }

    /// Get the file extension for source files.
    pub fn source_extension(&self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::Python => "py",
            Self::Go => "go",
            Self::Template => "",
        }
    }

    /// Get the test command for this project type.
    pub fn test_command(&self) -> Vec<&'static str> {
        match self {
            Self::Rust => vec!["cargo", "test"],
            Self::Python => vec!["python3", "-m", "pytest", "-v"],
            Self::Go => vec!["go", "test", "-v", "./..."],
            Self::Template => vec!["cargo", "test"],
        }
    }

    /// Get the check/compile command for this project type.
    pub fn check_command(&self) -> Vec<&'static str> {
        match self {
            Self::Rust => vec!["cargo", "check"],
            Self::Python => vec!["python3", "-m", "py_compile"],
            Self::Go => vec!["go", "build", "./..."],
            Self::Template => vec!["cargo", "check"],
        }
    }
}

fn init_scaffold_git_repo(dir: &Path, initial_commit: bool) {
    let _ = Command::new("git")
        .sanitized_git(dir, GitScope::Internal)
        .args(["init", "-q"])
        .current_dir(dir)
        .output();

    if initial_commit {
        let _ = Command::new("git")
            .sanitized_git(dir, GitScope::Internal)
            .args(["add", "-A"])
            .current_dir(dir)
            .output();

        let _ = Command::new("git")
            .sanitized_git(dir, GitScope::Internal)
            .args([
                "-c",
                "user.name=selfware",
                "-c",
                "user.email=selfware@local",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "init",
            ])
            .current_dir(dir)
            .output();
    }
}

/// Scaffolds a new greenfield Rust project.
pub fn scaffold_rust(name: &str, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("src"))?;

    // Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
"#,
        name = name.replace("-", "_")
    );
    crate::session::checkpoint::write_bytes_atomically(
        &dir.join("Cargo.toml"),
        cargo_toml.as_bytes(),
        0o644,
    )?;

    // src/lib.rs
    crate::session::checkpoint::write_bytes_atomically(
        &dir.join("src/lib.rs"),
        b"// Implement here\n",
        0o644,
    )?;

    init_scaffold_git_repo(dir, true);

    Ok(())
}

/// Scaffolds a new Python project.
pub fn scaffold_python(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    init_scaffold_git_repo(dir, false);

    Ok(())
}

/// Scaffolds a new Go project.
pub fn scaffold_go(module: &str, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    // go.mod
    let go_mod = format!(
        r#"module {module}

go 1.21
"#,
        module = module
    );
    crate::session::checkpoint::write_bytes_atomically(
        &dir.join("go.mod"),
        go_mod.as_bytes(),
        0o644,
    )?;

    init_scaffold_git_repo(dir, true);

    Ok(())
}

/// Copies a template project.
pub fn scaffold_template(template: &str, templates_dir: &Path, dir: &Path) -> std::io::Result<()> {
    if template.is_empty()
        || template.contains('/')
        || template.contains('\\')
        || template.contains('\0')
        || template.contains("..")
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid template name: {}", template),
        ));
    }
    let src = templates_dir.join(template);
    if !src.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Template not found: {}", src.display()),
        ));
    }

    // Copy template
    copy_dir_all(&src, dir)?;

    // Clean up build artifacts and any embedded git metadata
    let _ = std::fs::remove_dir_all(dir.join(".git"));
    let _ = std::fs::remove_dir_all(dir.join("target"));
    let _ = std::fs::remove_file(dir.join("Cargo.lock"));
    let _ = std::fs::remove_dir_all(dir.join("node_modules"));

    init_scaffold_git_repo(dir, true);

    Ok(())
}

/// Recursively copy a directory, refusing symlinks.
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    let src_meta = std::fs::symlink_metadata(&src)?;
    if src_meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Refusing to copy symlink template directory: {}",
                src.as_ref().display()
            ),
        ));
    }
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Refusing to copy symlink in template: {}",
                    entry.path().display()
                ),
            ));
        } else if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Test project evaluation result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectStatus {
    /// All tests passed.
    Green,
    /// Some tests passed.
    Partial,
    /// Compiles but no tests or tests failed.
    Compiles,
    /// Code was written but doesn't compile.
    Wrote,
    /// Failed to produce code.
    Fail,
}

impl ProjectStatus {
    /// Determine status from test results.
    pub fn from_results(compiles: bool, passed: usize, failed: usize, src_lines: usize) -> Self {
        if compiles && passed > 0 && failed == 0 {
            Self::Green
        } else if compiles && passed > 0 {
            Self::Partial
        } else if compiles {
            Self::Compiles
        } else if src_lines > 2 {
            Self::Wrote
        } else {
            Self::Fail
        }
    }
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Green => write!(f, "GREEN"),
            Self::Partial => write!(f, "PARTIAL"),
            Self::Compiles => write!(f, "COMPILES"),
            Self::Wrote => write!(f, "WROTE"),
            Self::Fail => write!(f, "FAIL"),
        }
    }
}

/// Results from evaluating a project.
#[derive(Debug, Clone)]
pub struct ProjectResult {
    pub name: String,
    pub status: ProjectStatus,
    pub duration_secs: u64,
    pub steps: usize,
    pub src_lines: usize,
    pub compiles: bool,
    pub tests_passed: usize,
    pub tests_failed: usize,
    pub outcome_label: String,
}

#[cfg(test)]
#[path = "../../../tests/unit/bench_harness/long_running/project/project_test.rs"]
mod tests;
