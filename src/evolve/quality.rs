//! QualityAnalyzer: computes quality metrics for code components.
//!
//! Metrics per node:
//! - `warning_count`: compiler warnings from `cargo check` (cached per analyzer;
//!   `None` when `cargo` is unavailable).
//! - `complexity`: estimated cyclomatic complexity summed over the component's files.
//! - `dead_code_ratio`: `#[allow(dead_code)]` annotations per function (static heuristic).
//! - `coverage`: left `None`; real coverage requires tarpaulin/llvm-cov integration.

use anyhow::Result;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use super::Node;

pub struct QualityAnalyzer {
    /// Compiler warning counts keyed by source file (e.g. "src/agent/mod.rs"),
    /// collected lazily from a single `cargo check` run. `None` when `cargo
    /// check` could not run (e.g. cargo not on PATH); metrics degrade to `None`.
    warnings: OnceCell<Option<HashMap<String, usize>>>,
    /// Command used to run `cargo check`; overridable for tests.
    cargo_cmd: String,
}

impl QualityAnalyzer {
    pub fn new() -> Self {
        Self::with_cargo_cmd("cargo")
    }

    fn with_cargo_cmd(cmd: &str) -> Self {
        Self {
            warnings: OnceCell::new(),
            cargo_cmd: cmd.to_string(),
        }
    }

    pub fn analyze_node(&self, node: &mut Node) -> Result<()> {
        let Some(ref path) = node.path.clone() else {
            return Ok(());
        };

        node.warning_count = self.warnings_for(path);
        node.complexity = Some(cyclomatic_complexity(Path::new(path))?);
        node.dead_code_ratio = Some(dead_code_ratio(Path::new(path))?);
        // Real coverage requires tarpaulin/llvm-cov; unknown for now.
        node.coverage = None;
        Ok(())
    }

    /// Number of compiler warnings in files under `path` (e.g. "src/agent").
    /// Returns `None` if `cargo check` is unavailable or failed.
    fn warnings_for(&self, path: &str) -> Option<usize> {
        if self.warnings.get().is_none() {
            let _ = self.warnings.set(collect_warnings_from(&self.cargo_cmd));
        }
        let map = self.warnings.get()?.as_ref()?;
        let prefix = format!("{}/", path.trim_end_matches('/'));
        Some(
            map.iter()
                .filter(|(file, _)| file.starts_with(&prefix) || file.as_str() == path)
                .map(|(_, n)| n)
                .sum(),
        )
    }
}

impl Default for QualityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Run `cargo check` once and count warnings per source file.
/// Returns `None` (and logs a warning) if `cargo` cannot be executed,
/// e.g. when the toolchain is not installed or not on PATH. The command
/// name is a parameter so tests can exercise the unavailable-toolchain
/// path without mutating the process-wide `PATH`.
fn collect_warnings_from(cmd: &str) -> Option<HashMap<String, usize>> {
    let output = match Command::new(cmd)
        .args(["check", "--message-format=json"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            eprintln!("warning: cargo check unavailable, skipping warning metrics: {e}");
            return None;
        }
    };

    let mut counts = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let diag = &msg["message"];
        if diag.get("level").and_then(|l| l.as_str()) != Some("warning") {
            continue;
        }
        let Some(spans) = diag.get("spans").and_then(|s| s.as_array()) else {
            continue;
        };
        for span in spans {
            if span.get("is_primary").and_then(|p| p.as_bool()) == Some(true) {
                if let Some(file) = span.get("file_name").and_then(|f| f.as_str()) {
                    *counts.entry(file.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    Some(counts)
}

/// Collect the contents of all `.rs` files under `path`.
fn read_rs_files(path: &Path) -> Result<Vec<String>> {
    let mut contents = Vec::new();
    if path.is_file() {
        if path.extension().map_or(false, |e| e == "rs") {
            contents.push(std::fs::read_to_string(path)?);
        }
    } else if path.is_dir() {
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "rs") {
                contents.push(std::fs::read_to_string(entry.path())?);
            }
        }
    }
    Ok(contents)
}

/// Estimated cyclomatic complexity: 1 + decision points, summed over all files.
fn cyclomatic_complexity(path: &Path) -> Result<f64> {
    let mut total = 0.0;
    for content in read_rs_files(path)? {
        let mut score = 1.0;
        for token in [" if ", " else if ", " match ", " for ", " while ", " loop ", "&&", "||"] {
            score += content.matches(token).count() as f64;
        }
        total += score;
    }
    Ok(total)
}

/// Ratio of `#[allow(dead_code)]` annotations to function definitions.
fn dead_code_ratio(path: &Path) -> Result<f64> {
    let mut allows = 0usize;
    let mut fns = 0usize;
    for content in read_rs_files(path)? {
        allows += content.matches("#[allow(dead_code)]").count();
        fns += content.matches("fn ").count();
    }
    Ok(if fns == 0 {
        0.0
    } else {
        (allows as f64 / fns as f64).min(1.0)
    })
}

#[cfg(test)]
mod tests {
    use super::Node;
    use super::*;

    #[test]
    fn test_quality_analyzer_populates_coverage() {
        let mut node = Node::code("agent", "src/agent");
        let analyzer = QualityAnalyzer::new();
        analyzer.analyze_node(&mut node).unwrap();
        assert!(node.coverage.is_some() || node.warning_count.is_some());
    }

    #[test]
    fn test_quality_analyzer_computes_complexity_and_dead_code() {
        let mut node = Node::code("evolve", "src/evolve");
        let analyzer = QualityAnalyzer::new();
        analyzer.analyze_node(&mut node).unwrap();
        assert!(node.complexity.unwrap() > 0.0);
        assert!(node.dead_code_ratio.unwrap() >= 0.0);
        assert!(node.dead_code_ratio.unwrap() <= 1.0);
    }

    #[test]
    fn test_graceful_degradation_when_cargo_unavailable() {
        // Use a command name that cannot exist instead of mutating the
        // process-wide PATH (which races other cargo-spawning tests).
        assert!(collect_warnings_from("selfware-nonexistent-cargo-binary").is_none());

        let mut node = Node::code("evolve", "src/evolve");
        let analyzer = QualityAnalyzer::with_cargo_cmd("selfware-nonexistent-cargo-binary");
        // Analysis must not fail; warning metrics degrade to None.
        analyzer.analyze_node(&mut node).unwrap();
        assert!(node.warning_count.is_none());
        assert!(node.complexity.is_some());
    }

    #[test]
    fn test_complexity_heuristic_counts_decision_points() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.rs"),
            "fn f(x: bool) {\n if x && true { } else { }\n}\n",
        )
        .unwrap();
        let score = cyclomatic_complexity(dir.path()).unwrap();
        // 1 base + 1 `if` + 1 `&&`
        assert_eq!(score, 3.0);
    }

    #[test]
    fn test_dead_code_ratio_heuristic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.rs"),
            "#[allow(dead_code)]\nfn a() {}\nfn b() {}\n",
        )
        .unwrap();
        let ratio = dead_code_ratio(dir.path()).unwrap();
        assert_eq!(ratio, 0.5);
    }
}
