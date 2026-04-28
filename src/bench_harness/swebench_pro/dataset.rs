//! Loads SWE-bench Pro instances from HuggingFace.
//!
//! HF parquet parsing in pure Rust is heavyweight (`arrow`/`parquet` crates pull
//! a lot of transitive deps), and Python's `datasets` library is already a hard
//! prerequisite for the existing harness — both selfware bench callers run on
//! the same workstation that already has it installed.  We shell out to a tiny
//! inline Python program that emits one JSON object per row to stdout, then
//! parse those rows with `serde_json`.  This matches the Python harness's
//! behaviour exactly without forcing a new heavyweight Rust dep.

use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

/// A single SWE-bench Pro problem.  We keep the schema permissive — Scale AI
/// occasionally adds optional fields, and we don't want to break on them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub instance_id: String,
    pub repo: String,
    pub base_commit: String,
    pub problem_statement: String,
    /// May be a JSON-string, a list of strings, or absent.
    #[serde(default)]
    pub fail_to_pass: serde_json::Value,
    #[serde(default)]
    pub selected_test_files_to_run: serde_json::Value,
    #[serde(default)]
    pub repo_language: Option<String>,
    /// Anything else carried by the row.  Preserved for `instance.json`.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

const LOADER_PY: &str = r#"
import json, sys
try:
    from datasets import load_dataset
except Exception as e:
    sys.stderr.write(f"failed to import datasets: {e}\n")
    sys.exit(2)
ds = load_dataset("ScaleAI/SWE-bench_Pro", split="test")
for row in ds:
    sys.stdout.write(json.dumps(dict(row), default=str) + "\n")
"#;

/// Load and filter SWE-bench Pro test instances.
///
/// * `instance_ids` — when non-empty, only rows whose `instance_id` is in this
///   set are returned (in the order they appear in the dataset).
/// * `limit` — when `instance_ids` is empty, picks the `limit` rows with the
///   shortest `problem_statement` (matching the Python harness's "easiest first"
///   heuristic for sanity runs).
pub fn load_instances(instance_ids: &[String], limit: usize) -> Result<Vec<Instance>> {
    let raw = run_loader_script()?;
    let mut all: Vec<Instance> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Instance>)
        .collect::<Result<Vec<_>, _>>()
        .context("parsing SWE-bench Pro JSONL rows")?;

    if !instance_ids.is_empty() {
        let wanted: std::collections::HashSet<&str> =
            instance_ids.iter().map(String::as_str).collect();
        all.retain(|i| wanted.contains(i.instance_id.as_str()));
        return Ok(all);
    }

    all.sort_by_key(|i| i.problem_statement.len());
    all.truncate(limit);
    Ok(all)
}

fn run_loader_script() -> Result<String> {
    let py = std::env::var("SELFWARE_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let out = Command::new(&py)
        .arg("-c")
        .arg(LOADER_PY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("spawning {}", py))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "SWE-bench Pro loader failed (status={:?}):\n{}",
            out.status.code(),
            stderr
        );
    }
    String::from_utf8(out.stdout).map_err(|e| anyhow!("non-UTF8 dataset output: {}", e))
}

/// Coerce a possibly-stringified JSON list field into `Vec<String>`.
///
/// Mirrors the Python harness which handles both raw lists and JSON-encoded
/// strings interchangeably.
pub fn coerce_string_list(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        serde_json::Value::String(s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => vec![s.clone()],
        },
        serde_json::Value::Null => Vec::new(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerce_handles_raw_array() {
        assert_eq!(
            coerce_string_list(&json!(["a", "b"])),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn coerce_handles_stringified_array() {
        assert_eq!(
            coerce_string_list(&json!("[\"a\", \"b\"]")),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn coerce_handles_plain_string() {
        assert_eq!(coerce_string_list(&json!("solo")), vec!["solo".to_string()]);
    }

    #[test]
    fn coerce_handles_null() {
        assert!(coerce_string_list(&json!(null)).is_empty());
    }
}
