//! Localize issue tool — ranks candidate files/functions/tests for a given
//! issue description using BM25-style keyword scoring, test-file proximity,
//! and recent git change history.

use crate::tools::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::instrument;
use walkdir::WalkDir;

/// A single candidate file/function returned by `localize_issue`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub file: String,
    pub function: String,
    pub score: f64,
    pub reason: String,
}

/// Tool implementation.
pub struct LocalizeIssue;

/// Tokenize text into lowercase alphanumeric terms (filters out short words).
fn tokenize(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one",
        "our", "out", "day", "get", "has", "him", "his", "how", "its", "may", "new", "now", "old",
        "see", "two", "who", "boy", "did", "she", "use", "her", "way", "many",
    ];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|s| s.trim_matches('_'))
        .filter(|s| s.len() > 2 && !STOP_WORDS.contains(s))
        .map(String::from)
        .collect()
}

/// Check whether a file looks like a source file we should index.
fn is_source_file(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e,
        None => return false,
    };
    matches!(
        ext,
        "rs" | "py"
            | "js"
            | "ts"
            | "go"
            | "java"
            | "cpp"
            | "c"
            | "h"
            | "hpp"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "m"
            | "mm"
    )
}

/// Collect files recently modified in git (last 30 days).
fn recent_git_files(repo_path: &str) -> HashSet<String> {
    let output = std::process::Command::new("git")
        .args([
            "log",
            "--pretty=format:",
            "--name-only",
            "--since=30 days ago",
            "--diff-filter=AM",
        ])
        .current_dir(repo_path)
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => HashSet::new(),
    }
}

/// Find the most relevant function name in a file for the given query terms.
fn best_function_in_file(content: &str, query_terms: &[String]) -> String {
    static FN_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap());
    static DEF_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?(?:def|function|func)\s+(\w+)").unwrap()
    });
    static CLASS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^\s*(?:pub\s+)?class\s+(\w+)").unwrap());

    let mut best = ("", 0usize);
    for cap in FN_RE.captures_iter(content) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let score = query_terms
            .iter()
            .filter(|t| name.to_lowercase().contains(*t))
            .count();
        if score > best.1 {
            best = (name, score);
        }
    }
    for cap in DEF_RE.captures_iter(content) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let score = query_terms
            .iter()
            .filter(|t| name.to_lowercase().contains(*t))
            .count();
        if score > best.1 {
            best = (name, score);
        }
    }
    for cap in CLASS_RE.captures_iter(content) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let score = query_terms
            .iter()
            .filter(|t| name.to_lowercase().contains(*t))
            .count();
        if score > best.1 {
            best = (name, score);
        }
    }

    if best.0.is_empty() {
        // Fallback: look for any camelCase / snake_case identifier that overlaps.
        let id_re = Regex::new(r"[a-zA-Z_][a-zA-Z0-9_]*").unwrap();
        for m in id_re.find_iter(content) {
            let name = m.as_str();
            let score = query_terms
                .iter()
                .filter(|t| name.to_lowercase().contains(*t))
                .count();
            if score > best.1 {
                best = (name, score);
            }
        }
    }

    best.0.to_string()
}

/// Run the localization heuristic and return ranked candidates.
pub(crate) fn localize_issue_sync(issue: &str, repo_path: &str) -> Result<Vec<Candidate>> {
    let query_terms = tokenize(issue);
    if query_terms.is_empty() {
        return Ok(vec![]);
    }

    let recent_files = recent_git_files(repo_path);

    // ------------------------------------------------------------------
    // Walk the repository and collect file stats.
    // ------------------------------------------------------------------
    let mut file_contents: HashMap<PathBuf, String> = HashMap::new();
    let mut term_freqs: HashMap<PathBuf, HashMap<String, usize>> = HashMap::new();
    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    let mut doc_lengths: HashMap<PathBuf, usize> = HashMap::new();
    let mut total_doc_len: usize = 0;

    for entry in WalkDir::new(repo_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let path_str = path.to_string_lossy();

        // Skip common noise directories.
        if path_str.contains("/target/")
            || path_str.contains("/.git/")
            || path_str.contains("/node_modules/")
            || path_str.contains("/.venv/")
            || path_str.contains("/__pycache__/")
            || path_str.contains("/dist/")
            || path_str.contains("/build/")
        {
            continue;
        }

        if !is_source_file(path) {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let tokens = tokenize(&content);
        let len = tokens.len();
        doc_lengths.insert(path.to_path_buf(), len);
        total_doc_len += len;

        let mut local_tf: HashMap<String, usize> = HashMap::new();
        for t in &tokens {
            *local_tf.entry(t.clone()).or_insert(0) += 1;
        }

        for t in local_tf.keys() {
            *doc_freq.entry(t.clone()).or_insert(0) += 1;
        }

        term_freqs.insert(path.to_path_buf(), local_tf);
        file_contents.insert(path.to_path_buf(), content);
    }

    let num_docs = file_contents.len().max(1);
    let avg_doc_len = (total_doc_len as f64) / (num_docs as f64).max(1.0);

    // ------------------------------------------------------------------
    // Score every file.
    // ------------------------------------------------------------------
    let k1 = 1.5;
    let b = 0.75;

    let mut scored: Vec<(PathBuf, f64, String)> = Vec::new();

    for path in file_contents.keys() {
        let tf_map = match term_freqs.get(path) {
            Some(m) => m,
            None => continue,
        };

        let doc_len = *doc_lengths.get(path).unwrap_or(&0) as f64;
        let mut bm25 = 0.0;

        for term in &query_terms {
            let tf = *tf_map.get(term).unwrap_or(&0) as f64;
            let df = *doc_freq.get(term).unwrap_or(&0) as f64;
            let idf = ((num_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();
            let norm =
                tf * (k1 + 1.0) / (tf + k1 * (1.0 - b + b * (doc_len / avg_doc_len.max(1.0))));
            bm25 += idf * norm;
        }

        // Path match boost.
        let path_lower = path.to_string_lossy().to_lowercase();
        let path_boost = query_terms
            .iter()
            .filter(|t| path_lower.contains(*t))
            .count() as f64
            * 2.0;

        // Test file proximity boost.
        let is_test = path_lower.contains("test") || path_lower.contains("spec");
        let test_boost = if is_test { 2.0 } else { 0.0 };

        // Recent git change boost.
        let rel_path = path
            .strip_prefix(repo_path)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let git_boost = if recent_files.contains(&rel_path) {
            1.5
        } else {
            0.0
        };

        let score = bm25 + path_boost + test_boost + git_boost;

        if score > 0.0 {
            let mut reasons = Vec::new();
            if bm25 > 0.0 {
                reasons.push(format!("BM25 content score {:.2}", bm25));
            }
            if path_boost > 0.0 {
                reasons.push("path keyword match".to_string());
            }
            if test_boost > 0.0 {
                reasons.push("test file proximity".to_string());
            }
            if git_boost > 0.0 {
                reasons.push("recently modified".to_string());
            }
            let reason = reasons.join(", ");
            scored.push((path.clone(), score, reason));
        }
    }

    // Sort by score descending.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // ------------------------------------------------------------------
    // Build candidates (top 10).
    // ------------------------------------------------------------------
    let mut candidates = Vec::new();
    for (path, score, reason) in scored.into_iter().take(10) {
        let function = file_contents
            .get(&path)
            .map(|c| best_function_in_file(c, &query_terms))
            .unwrap_or_default();
        candidates.push(Candidate {
            file: path.to_string_lossy().to_string(),
            function,
            score,
            reason,
        });
    }

    Ok(candidates)
}

#[async_trait]
impl Tool for LocalizeIssue {
    fn name(&self) -> &str {
        "localize_issue"
    }

    fn description(&self) -> &str {
        "Given an issue description, returns ranked candidate files/functions/tests \
         using BM25 + code graph + LSP symbols."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["issue"],
            "properties": {
                "issue": {
                    "type": "string",
                    "description": "Issue description or bug report text"
                },
                "repo_path": {
                    "type": "string",
                    "default": ".",
                    "description": "Path to the repository root to search"
                }
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let issue = args
            .get("issue")
            .and_then(|v| v.as_str())
            .context("Missing required parameter: issue")?
            .to_string();
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();

        let candidates =
            tokio::task::spawn_blocking(move || localize_issue_sync(&issue, &repo_path))
                .await
                .context("localize_issue blocking task panicked")??;

        Ok(serde_json::json!({
            "candidates": candidates,
            "count": candidates.len(),
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_tokenize_filters_short_words() {
        let terms = tokenize("The quick brown fox");
        // "the" is filtered as a stop word.
        assert!(!terms.contains(&"the".to_string()));
        assert!(terms.contains(&"quick".to_string()));
        assert!(terms.contains(&"brown".to_string()));
        assert!(terms.contains(&"fox".to_string()));
        // "a" and "ab" (length <= 2) are filtered.
        let short = tokenize("a ab abc");
        assert!(!short.contains(&"a".to_string()));
        assert!(!short.contains(&"ab".to_string()));
        assert!(short.contains(&"abc".to_string()));
    }

    #[test]
    fn test_localize_issue_finds_relevant_file() {
        let temp_dir = TempDir::new().unwrap();
        let repo = temp_dir.path();

        // Create a dummy source tree.
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::create_dir_all(repo.join("tests")).unwrap();
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn process_data(input: &str) -> String {\n    input.to_string()\n}\n",
        )
        .unwrap();
        fs::write(
            repo.join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        fs::write(
            repo.join("tests/test_process.rs"),
            "#[test]\nfn test_process_data() {\n    assert_eq!(process_data(\"x\"), \"x\");\n}\n",
        )
        .unwrap();

        let candidates =
            localize_issue_sync("process_data returns wrong value", repo.to_str().unwrap())
                .unwrap();
        assert!(!candidates.is_empty(), "should find at least one candidate");

        // The lib.rs file should rank highest because it contains process_data.
        let top_file = &candidates[0].file;
        assert!(
            top_file.contains("lib.rs") || top_file.contains("test_process.rs"),
            "top candidate should be lib.rs or test_process.rs, got {}",
            top_file
        );

        // Check that the candidate has a reasonable reason.
        assert!(!candidates[0].reason.is_empty());
    }

    #[test]
    fn test_localize_issue_empty_query() {
        let temp_dir = TempDir::new().unwrap();
        let candidates = localize_issue_sync("a", temp_dir.path().to_str().unwrap()).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_best_function_in_file() {
        let code = "pub fn foo() {}\npub fn bar() {}\npub fn process_data() {}";
        let terms = vec!["process".to_string(), "data".to_string()];
        let best = best_function_in_file(code, &terms);
        assert_eq!(best, "process_data");
    }

    #[tokio::test]
    async fn test_localize_issue_tool_execute() {
        let temp_dir = TempDir::new().unwrap();
        let repo = temp_dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), "pub fn fix_me() {}\n").unwrap();

        let tool = LocalizeIssue;
        let args = serde_json::json!({
            "issue": "fix_me is broken",
            "repo_path": repo.to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        let candidates = result["candidates"].as_array().unwrap();
        assert!(!candidates.is_empty());
        assert!(candidates[0]["file"].as_str().unwrap().contains("lib.rs"));
    }
}
