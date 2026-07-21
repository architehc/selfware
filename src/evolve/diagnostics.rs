//! Structured compiler, lint, and focused-test feedback for the evolve IDE.
//!
//! The runner deliberately exposes a fixed command set. Browser requests can
//! never supply executable names or arbitrary arguments.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;

const MAX_DIAGNOSTICS: usize = 500;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisKind {
    #[serde(rename = "cargo_check", alias = "check")]
    Check,
    Clippy,
    EvolveTests,
}

impl AnalysisKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "Cargo check",
            Self::Clippy => "Clippy",
            Self::EvolveTests => "Evolve tests",
        }
    }

    pub(crate) fn args(self) -> &'static [&'static str] {
        match self {
            Self::Check => &["check", "--all-targets", "--message-format=json"],
            Self::Clippy => &["clippy", "--all-targets", "--message-format=json"],
            Self::EvolveTests => &[
                "test",
                "--test",
                "evolve",
                "--message-format=json",
                "--",
                "--nocapture",
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSpan {
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub is_primary: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerDiagnostic {
    pub level: String,
    pub code: Option<String>,
    pub message: String,
    pub rendered: Option<String>,
    pub spans: Vec<DiagnosticSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub kind: AnalysisKind,
    pub label: String,
    pub command: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub errors: usize,
    pub warnings: usize,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub evidence_complete: bool,
}

#[derive(Debug, Clone)]
pub struct DiagnosticsEngine {
    project_root: PathBuf,
}

impl DiagnosticsEngine {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
        }
    }

    pub async fn run(&self, kind: AnalysisKind) -> Result<AnalysisReport> {
        let started = Instant::now();
        let args = kind.args();
        let output = Command::new("cargo")
            .args(args)
            .current_dir(&self.project_root)
            .env("CARGO_TERM_COLOR", "never")
            .output()
            .await
            .with_context(|| format!("failed to run {}", kind.label()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut diagnostics = Vec::new();
        let mut dropped = 0usize;

        for line in stdout.lines().chain(stderr.lines()) {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value.get("reason").and_then(Value::as_str) != Some("compiler-message") {
                continue;
            }
            let Some(message) = value.get("message") else {
                continue;
            };
            if diagnostics.len() >= MAX_DIAGNOSTICS {
                dropped += 1;
                continue;
            }
            diagnostics.push(parse_diagnostic(message));
        }

        let errors = diagnostics.iter().filter(|d| d.level == "error").count();
        let warnings = diagnostics.iter().filter(|d| d.level == "warning").count();

        Ok(AnalysisReport {
            kind,
            label: kind.label().to_string(),
            command: std::iter::once("cargo".to_string())
                .chain(args.iter().map(|arg| (*arg).to_string()))
                .collect(),
            success: output.status.success(),
            exit_code: output.status.code(),
            duration_ms: started.elapsed().as_millis() as u64,
            diagnostics,
            errors,
            warnings,
            stdout_tail: tail(&stdout, MAX_OUTPUT_BYTES),
            stderr_tail: tail(&stderr, MAX_OUTPUT_BYTES),
            evidence_complete: dropped == 0,
        })
    }
}

fn parse_diagnostic(message: &Value) -> CompilerDiagnostic {
    let spans = message
        .get("spans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|span| {
            Some(DiagnosticSpan {
                file: span.get("file_name")?.as_str()?.to_string(),
                line_start: span.get("line_start")?.as_u64()? as usize,
                line_end: span.get("line_end")?.as_u64()? as usize,
                column_start: span.get("column_start")?.as_u64()? as usize,
                column_end: span.get("column_end")?.as_u64()? as usize,
                is_primary: span
                    .get("is_primary")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                label: span
                    .get("label")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect();

    CompilerDiagnostic {
        level: message
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        code: message
            .get("code")
            .and_then(|code| code.get("code"))
            .and_then(Value::as_str)
            .map(str::to_string),
        message: message
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("compiler diagnostic")
            .to_string(),
        rendered: message
            .get("rendered")
            .and_then(Value::as_str)
            .map(str::to_string),
        spans,
    }
}

fn tail(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len() - max_bytes;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    format!("[output truncated]\n{}", &value[start..])
}
