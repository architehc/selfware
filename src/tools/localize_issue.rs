//! Localize issue tool — ranks candidate files/functions/tests for a given
//! issue description using BM25-style keyword scoring, test-file proximity,
//! and recent git change history.

use crate::config::SafetyConfig;
use crate::safety::git_exec::{GitScope, SanitizedGitExt};
use crate::tools::file::{resolve_safety_config, validate_tool_path};
use crate::tools::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tracing::instrument;
use walkdir::WalkDir;

/// Resource ceiling for a single source artifact. Larger files are skipped.
pub(crate) const MAX_LOCALIZE_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Aggregate bytes read from source descriptors during one localization pass.
pub(crate) const MAX_LOCALIZE_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
/// Source-file candidates retained for descriptor checks and ranking during
/// one localization pass.
pub(crate) const MAX_LOCALIZE_SOURCE_FILES: usize = 4_096;
/// Repository entries visited before the walk stops. This also bounds
/// repositories containing huge numbers of non-source files.
pub(crate) const MAX_LOCALIZE_WALK_ENTRIES: usize = 32_768;
/// Terms inspected from one source artifact for ranking. Files beyond this
/// ceiling remain in the provenance census but are omitted from ranking.
pub(crate) const MAX_LOCALIZE_FILE_TOKENS: usize = 64 * 1024;
/// Terms inspected across one localization pass. This bounds tokenizer work
/// and the aggregate term-frequency maps retained for BM25 scoring.
pub(crate) const MAX_LOCALIZE_TOTAL_TOKENS: usize = 256 * 1024;
/// Maximum issue text accepted for ranking. The tool reports an error rather
/// than silently ranking from a prefix of a larger query.
pub(crate) const MAX_LOCALIZE_QUERY_BYTES: usize = 64 * 1024;
/// Maximum normalized query tokens, including repeats used by BM25 scoring.
pub(crate) const MAX_LOCALIZE_QUERY_TOKENS: usize = 4 * 1024;
/// Maximum distinct normalized terms retained from one query.
pub(crate) const MAX_LOCALIZE_QUERY_UNIQUE_TERMS: usize = 1024;
/// Recent-history output is only a ranking hint, so discard it rather than
/// allowing an adversarial git history to allocate without bound.
const MAX_RECENT_GIT_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RECENT_GIT_COMMITS: &str = "1000";

#[derive(Debug, Clone, Copy)]
struct LocalizationLimits {
    file_bytes: u64,
    total_bytes: u64,
    source_files: usize,
    walk_entries: usize,
    file_tokens: usize,
    total_tokens: usize,
    query_bytes: usize,
    query_tokens: usize,
    query_unique_terms: usize,
}

const DEFAULT_LOCALIZATION_LIMITS: LocalizationLimits = LocalizationLimits {
    file_bytes: MAX_LOCALIZE_FILE_BYTES,
    total_bytes: MAX_LOCALIZE_TOTAL_BYTES,
    source_files: MAX_LOCALIZE_SOURCE_FILES,
    walk_entries: MAX_LOCALIZE_WALK_ENTRIES,
    file_tokens: MAX_LOCALIZE_FILE_TOKENS,
    total_tokens: MAX_LOCALIZE_TOTAL_TOKENS,
    query_bytes: MAX_LOCALIZE_QUERY_BYTES,
    query_tokens: MAX_LOCALIZE_QUERY_TOKENS,
    query_unique_terms: MAX_LOCALIZE_QUERY_UNIQUE_TERMS,
};

/// One source artifact admitted to the localization corpus. Paths use `/`
/// separators and are relative to the requested repository root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalizationCensusFile {
    pub relative_path: String,
    pub sha256: String,
    pub byte_len: u64,
}

/// Provenance for a localization pass. The manifest digest commits to every
/// admitted `(relative_path, content_sha256)` pair in path order.
#[derive(Debug)]
pub(crate) struct LocalizationCensus {
    pub candidates: Vec<Candidate>,
    pub eligible_relative_paths: BTreeSet<String>,
    pub files: Vec<LocalizationCensusFile>,
    pub manifest_sha256: String,
    /// At least one walk/file/byte ceiling limited the source census.
    pub truncated: bool,
    /// At least one admitted source artifact could not be ranked completely
    /// within the per-file or aggregate token ceiling.
    pub ranking_truncated: bool,
    /// Walk failures encountered while enumerating the bounded corpus.
    pub walk_errors: usize,
    /// Files discovered as source but not admitted because their checked
    /// descriptor could not be read or proved stable and single-linked.
    pub read_errors: usize,
    pub scanned_bytes: u64,
}

impl LocalizationCensus {
    pub(crate) fn census_complete(&self) -> bool {
        !self.truncated && self.walk_errors == 0 && self.read_errors == 0
    }
}

/// A single candidate file/function returned by `localize_issue`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub file: String,
    pub function: String,
    pub score: f64,
    pub reason: String,
}

/// Tool implementation.
#[derive(Default)]
pub struct LocalizeIssue {
    /// Per-instance safety config for path-policy enforcement; falls back to
    /// the process-global config when `None`.
    pub safety_config: Option<SafetyConfig>,
}

impl LocalizeIssue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

const LOCALIZE_STOP_WORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one",
    "our", "out", "day", "get", "has", "him", "his", "how", "its", "may", "new", "now", "old",
    "see", "two", "who", "boy", "did", "she", "use", "her", "way", "many",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum LocalizeQueryLimitError {
    #[error("localize_issue query is {actual} bytes; maximum is {limit}")]
    Bytes { actual: usize, limit: usize },
    #[error("localize_issue query has more than {limit} normalized tokens")]
    Tokens { limit: usize },
    #[error("localize_issue query has more than {limit} distinct normalized terms")]
    UniqueTerms { limit: usize },
}

fn normalize_token(raw_term: &str) -> Option<String> {
    let normalized = raw_term.trim_matches('_').to_lowercase();
    (normalized.len() > 2 && !LOCALIZE_STOP_WORDS.contains(&normalized.as_str()))
        .then_some(normalized)
}

/// Tokenize text into lowercase alphanumeric terms (filters out short words).
#[cfg(test)]
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(normalize_token)
        .collect()
}

fn tokenize_query(
    text: &str,
    limits: LocalizationLimits,
) -> std::result::Result<(Vec<String>, HashSet<String>), LocalizeQueryLimitError> {
    if text.len() > limits.query_bytes {
        return Err(LocalizeQueryLimitError::Bytes {
            actual: text.len(),
            limit: limits.query_bytes,
        });
    }

    let mut tokens = Vec::new();
    let mut unique_terms = HashSet::new();
    for token in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(normalize_token)
    {
        if tokens.len() >= limits.query_tokens {
            return Err(LocalizeQueryLimitError::Tokens {
                limit: limits.query_tokens,
            });
        }
        if !unique_terms.contains(&token) {
            if unique_terms.len() >= limits.query_unique_terms {
                return Err(LocalizeQueryLimitError::UniqueTerms {
                    limit: limits.query_unique_terms,
                });
            }
            unique_terms.insert(token.clone());
        }
        tokens.push(token);
    }
    Ok((tokens, unique_terms))
}

struct TermFrequencyScan {
    frequencies: HashMap<String, usize>,
    token_count: usize,
    complete: bool,
}

/// Count document length and query-term occurrences without materializing all
/// source tokens. Long terms that cannot equal a query term are never copied.
fn scan_term_frequencies(
    text: &str,
    query_terms: &HashSet<String>,
    token_limit: usize,
) -> TermFrequencyScan {
    let max_query_term_bytes = query_terms.iter().map(String::len).max().unwrap_or(0);
    let mut frequencies = HashMap::new();
    let mut token_count = 0usize;
    let mut complete = true;

    for raw_term in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let Some(normalized) = normalize_token(raw_term) else {
            continue;
        };
        if token_count >= token_limit {
            complete = false;
            break;
        }
        token_count += 1;

        if normalized.len() <= max_query_term_bytes && query_terms.contains(&normalized) {
            *frequencies.entry(normalized).or_insert(0) += 1;
        }
    }

    TermFrequencyScan {
        frequencies,
        token_count,
        complete,
    }
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
    let child = std::process::Command::new("git")
        .sanitized_git(repo_path, GitScope::Internal)
        .args([
            "log",
            "-z",
            "--pretty=format:",
            "--name-only",
            "--since=30 days ago",
            "--max-count",
            MAX_RECENT_GIT_COMMITS,
            "--diff-filter=AM",
        ])
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();

    let Ok(mut child) = child else {
        return HashSet::new();
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return HashSet::new();
    };
    let mut bounded = Vec::new();
    let read_result = stdout
        .take(MAX_RECENT_GIT_OUTPUT_BYTES + 1)
        .read_to_end(&mut bounded);
    if read_result.is_err() || bounded.len() as u64 > MAX_RECENT_GIT_OUTPUT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return HashSet::new();
    }
    let Ok(status) = child.wait() else {
        return HashSet::new();
    };
    if !status.success() {
        return HashSet::new();
    }

    String::from_utf8_lossy(&bounded)
        .split('\0')
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| chunk.to_string())
        .collect()
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
///
/// `policy` is the per-file path policy: when `Some`, every walked source must
/// pass a descriptor-backed open (so a denied subdirectory or symlink swap
/// inside the validated repo root is never read); standalone helpers pass
/// `None` and receive the same protected-path defaults with an unrestricted
/// repository allow-list.
pub(crate) fn localize_issue_sync(
    issue: &str,
    repo_path: &str,
    policy: Option<&SafetyConfig>,
) -> Result<Vec<Candidate>> {
    Ok(localize_issue_with_limits(
        issue,
        repo_path,
        policy,
        DEFAULT_LOCALIZATION_LIMITS,
        true,
        false,
    )?
    .candidates)
}

/// Run localization and retain a bounded, content-addressed census of
/// the source artifacts admitted to the ranking corpus. This companion API
/// lets a caller bind a plan to exactly the corpus used for activation while the
/// legacy [`localize_issue_sync`] API remains unchanged. It deliberately omits
/// git-recency scoring so capability-constrained callers do not spawn a
/// subprocess merely to discover their read-only source corpus.
pub(crate) fn localize_issue_with_census_sync(
    issue: &str,
    repo_path: &str,
    policy: Option<&SafetyConfig>,
) -> Result<LocalizationCensus> {
    localize_issue_with_limits(
        issue,
        repo_path,
        policy,
        DEFAULT_LOCALIZATION_LIMITS,
        false,
        false,
    )
}

/// Recompute source-corpus provenance without a task query, git subprocess, or
/// ranking-dependent input. Callers can compare `manifest_sha256` and the
/// eligible path set with a prior activation census.
pub(crate) fn recompute_localization_census_sync(
    repo_path: &str,
    policy: Option<&SafetyConfig>,
) -> Result<LocalizationCensus> {
    localize_issue_with_limits(
        "",
        repo_path,
        policy,
        DEFAULT_LOCALIZATION_LIMITS,
        false,
        true,
    )
}

fn localize_issue_with_limits(
    issue: &str,
    repo_path: &str,
    policy: Option<&SafetyConfig>,
    limits: LocalizationLimits,
    include_git_recency: bool,
    census_only: bool,
) -> Result<LocalizationCensus> {
    let canonical_repo_root = validate_repository_root(Path::new(repo_path))?;
    let (query_terms, query_term_set) = tokenize_query(issue, limits)?;
    if query_terms.is_empty() && !census_only {
        return Ok(empty_census());
    }

    let recent_files = if include_git_recency {
        recent_git_files(repo_path)
    } else {
        HashSet::new()
    };
    let fallback_policy = SafetyConfig {
        // Standalone callers historically allowed an explicit repository path.
        // Keep that behavior while retaining the validator's built-in protected
        // system path checks and default credential-file deny list.
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    };
    let read_policy = policy.unwrap_or(&fallback_policy);
    let repo_root = Path::new(repo_path);

    // ------------------------------------------------------------------
    // Walk the repository and collect file stats.
    // ------------------------------------------------------------------
    let mut file_contents: HashMap<PathBuf, String> = HashMap::new();
    let mut term_freqs: HashMap<PathBuf, HashMap<String, usize>> = HashMap::new();
    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    let mut doc_lengths: HashMap<PathBuf, usize> = HashMap::new();
    let mut total_doc_len: usize = 0;
    let mut census_files = Vec::new();
    let mut scanned_bytes = 0u64;
    let mut truncated = false;
    let mut ranking_truncated = false;
    let mut walk_errors = 0usize;
    let mut read_errors = 0usize;
    let mut ranking_tokens_seen = 0usize;
    let mut source_candidates = BTreeMap::<String, PathBuf>::new();

    let walk = WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            // Directory pruning is shared with the repository walkers, but
            // candidate files must reach `read_source_file`: its descriptor
            // check records unsafe identities (including hard links) as read
            // errors so the census cannot claim completeness after silently
            // omitting them.
            entry.file_type().is_file() || crate::evolve::graph::retain_repository_entry(entry)
        });
    for (walk_entries_seen, entry) in walk.enumerate() {
        if walk_entries_seen >= limits.walk_entries {
            truncated = true;
            break;
        }

        let Ok(entry) = entry else {
            walk_errors = walk_errors.saturating_add(1);
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
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

        let Some(relative_path) = normalized_relative_path(repo_root, path) else {
            continue;
        };

        if source_candidates.len() < limits.source_files {
            source_candidates.insert(relative_path, path.to_path_buf());
            continue;
        }

        // Keep the lexicographically first bounded set without asking
        // WalkDir to sort an arbitrarily large directory in memory.
        truncated = true;
        let should_replace = source_candidates
            .last_key_value()
            .is_some_and(|(largest, _)| relative_path.as_str() < largest.as_str());
        if should_replace {
            source_candidates.pop_last();
            source_candidates.insert(relative_path, path.to_path_buf());
        }
    }

    let mut source_candidates = source_candidates.into_iter().collect::<Vec<_>>();
    source_candidates.sort_by(|left, right| left.0.cmp(&right.0));

    for (relative_path, path) in source_candidates {
        let path_str = path.to_string_lossy();
        let read = read_source_file(
            &path_str,
            &canonical_repo_root,
            &relative_path,
            read_policy,
            limits,
            &mut scanned_bytes,
        );
        let (content, content_sha256, byte_len) = match read {
            SourceRead::Content {
                content,
                sha256,
                byte_len,
            } => (content, sha256, byte_len),
            SourceRead::Excluded => continue,
            SourceRead::ReadError => {
                read_errors = read_errors.saturating_add(1);
                continue;
            }
            SourceRead::FileLimit => {
                truncated = true;
                continue;
            }
            SourceRead::TotalLimit => {
                truncated = true;
                break;
            }
        };

        census_files.push(LocalizationCensusFile {
            relative_path,
            sha256: content_sha256,
            byte_len,
        });
        if census_only {
            continue;
        }

        let remaining_tokens = limits.total_tokens.saturating_sub(ranking_tokens_seen);
        if remaining_tokens == 0 {
            ranking_truncated = true;
            continue;
        }
        let scan = scan_term_frequencies(
            &content,
            &query_term_set,
            limits.file_tokens.min(remaining_tokens),
        );
        ranking_tokens_seen = ranking_tokens_seen.saturating_add(scan.token_count);
        if !scan.complete {
            ranking_truncated = true;
            continue;
        }

        let len = scan.token_count;
        doc_lengths.insert(path.clone(), len);
        total_doc_len = total_doc_len.saturating_add(len);

        for term in scan.frequencies.keys() {
            *doc_freq.entry(term.clone()).or_insert(0) += 1;
        }

        term_freqs.insert(path.clone(), scan.frequencies);
        file_contents.insert(path, content);
    }

    census_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let eligible_relative_paths = census_files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<BTreeSet<_>>();
    let manifest_sha256 = census_manifest_digest(
        &census_files,
        truncated,
        walk_errors,
        read_errors,
        scanned_bytes,
    );

    let num_docs = file_contents.len().max(1);
    let avg_doc_len = (total_doc_len as f64) / (num_docs as f64).max(1.0);

    // ------------------------------------------------------------------
    // Score every file.
    // ------------------------------------------------------------------
    let k1 = 1.5;
    let b = 0.75;

    let mut scored: Vec<(PathBuf, f64, String)> = Vec::new();

    let mut paths = if query_terms.is_empty() {
        Vec::new()
    } else {
        file_contents.keys().cloned().collect::<Vec<_>>()
    };
    paths.sort();
    for path in paths {
        let tf_map = match term_freqs.get(&path) {
            Some(m) => m,
            None => continue,
        };

        let doc_len = *doc_lengths.get(&path).unwrap_or(&0) as f64;
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
            .unwrap_or(&path)
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
            scored.push((path, score, reason));
        }
    }

    // Sort by score descending, then by path for stable ties.
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

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

    Ok(LocalizationCensus {
        candidates,
        eligible_relative_paths,
        files: census_files,
        manifest_sha256,
        truncated,
        ranking_truncated,
        walk_errors,
        read_errors,
        scanned_bytes,
    })
}

enum SourceRead {
    Content {
        content: String,
        sha256: String,
        byte_len: u64,
    },
    Excluded,
    ReadError,
    FileLimit,
    TotalLimit,
}

fn read_source_file(
    path: &str,
    canonical_repo_root: &Path,
    relative_path: &str,
    policy: &SafetyConfig,
    limits: LocalizationLimits,
    scanned_bytes: &mut u64,
) -> SourceRead {
    // A policy-denied path is intentionally outside the corpus. Failures
    // after this lexical check are operational/integrity failures and make
    // completeness false rather than silently shrinking the corpus.
    if validate_tool_path(path, policy).is_err() {
        return SourceRead::Excluded;
    }
    let Ok(validated) = crate::tools::file::open_checked_regular_with_path(path, policy) else {
        return SourceRead::ReadError;
    };
    // Bind the opened descriptor to the exact lexical census entry. A parent
    // directory replaced by a symlink after WalkDir yielded the path must not
    // redirect the read elsewhere inside the repository (including private
    // state) while retaining the old public relative name.
    let expected_real_path = canonical_repo_root.join(relative_path);
    if validated.real_path != expected_real_path
        || crate::safety::source_context::path_contains_private_discovery_state(
            &validated.real_path,
        )
    {
        return SourceRead::ReadError;
    }
    let mut file = validated.file;
    let Ok(before) = crate::safety::source_context::validate_broad_discovery_file_handle(&file)
    else {
        return SourceRead::ReadError;
    };
    if before.len() > limits.file_bytes {
        return SourceRead::FileLimit;
    }

    let remaining = limits.total_bytes.saturating_sub(*scanned_bytes);
    // Reserve one byte beyond the descriptor size. It is an EOF/growth probe:
    // if the file grows after fstat, it is rejected instead of admitting a
    // prefix while the aggregate number of bytes read remains bounded.
    if before.len().saturating_add(1) > remaining {
        return SourceRead::TotalLimit;
    }
    let read_cap = before
        .len()
        .saturating_add(1)
        .min(limits.file_bytes.saturating_add(1))
        .min(remaining);
    let mut bytes = Vec::with_capacity(before.len() as usize);
    let result = (&mut file).take(read_cap).read_to_end(&mut bytes);
    *scanned_bytes = (*scanned_bytes).saturating_add(bytes.len() as u64);
    if result.is_err() {
        return SourceRead::ReadError;
    }
    if bytes.len() as u64 > limits.file_bytes {
        return SourceRead::FileLimit;
    }

    // Recheck the same descriptor. A truncate/append race must not turn the
    // bounded read into a silently accepted prefix.
    let descriptor_len = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => return SourceRead::ReadError,
    };
    if descriptor_len != bytes.len() as u64 {
        return SourceRead::ReadError;
    }
    let byte_len = bytes.len() as u64;
    let sha256 = sha256_hex(&bytes);
    let Ok(content) = String::from_utf8(bytes) else {
        return SourceRead::Excluded;
    };
    SourceRead::Content {
        content,
        sha256,
        byte_len,
    }
}

fn validate_repository_root(repo_root: &Path) -> Result<PathBuf> {
    if contains_private_root_component(repo_root) {
        anyhow::bail!(
            "localize_issue refuses private repository root {}",
            repo_root.display()
        );
    }

    let canonical = std::fs::canonicalize(repo_root)
        .with_context(|| format!("failed to resolve repository root {}", repo_root.display()))?;
    if contains_private_root_component(&canonical) {
        anyhow::bail!(
            "localize_issue refuses canonical private repository root {}",
            canonical.display()
        );
    }
    if !canonical.is_dir() {
        anyhow::bail!(
            "localize_issue repository root is not a directory: {}",
            canonical.display()
        );
    }
    Ok(canonical)
}

fn contains_private_root_component(path: &Path) -> bool {
    crate::safety::source_context::path_contains_private_discovery_state(path)
}

fn normalized_relative_path(repo_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(repo_root).ok()?;
    let mut components = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return None;
        };
        components.push(component.to_str()?.to_string());
    }
    if components.is_empty() {
        None
    } else {
        Some(components.join("/"))
    }
}

fn census_manifest_digest(
    files: &[LocalizationCensusFile],
    truncated: bool,
    walk_errors: usize,
    read_errors: usize,
    scanned_bytes: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"selfware.localize_issue.census.v3\0");
    hasher.update([u8::from(truncated)]);
    hasher.update((walk_errors as u64).to_be_bytes());
    hasher.update((read_errors as u64).to_be_bytes());
    hasher.update(scanned_bytes.to_be_bytes());
    hasher.update((files.len() as u64).to_be_bytes());
    for file in files {
        let path = file.relative_path.as_bytes();
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path);
        hasher.update(file.sha256.as_bytes());
        hasher.update(file.byte_len.to_be_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn empty_census() -> LocalizationCensus {
    LocalizationCensus {
        candidates: Vec::new(),
        eligible_relative_paths: BTreeSet::new(),
        files: Vec::new(),
        manifest_sha256: census_manifest_digest(&[], false, 0, 0, 0),
        truncated: false,
        ranking_truncated: false,
        walk_errors: 0,
        read_errors: 0,
        scanned_bytes: 0,
    }
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
        // A relative repo path resolves against the agent's workspace root.
        let repo_path = crate::tools::workspace_root::anchor(
            args.get("repo_path")
                .and_then(|v| v.as_str())
                .unwrap_or("."),
        );

        // Enforce the workspace path policy before walking the repo: the tool
        // reads every source file under `repo_path`, so the root must pass the
        // same checks as `file_read`. The resolved config is also applied
        // per-file during the walk so denied subdirectories leak nothing.
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&repo_path, &safety)?;

        let candidates = crate::tools::workspace_root::spawn_blocking(move || {
            localize_issue_sync(&issue, &repo_path, Some(&safety))
        })
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
#[path = "../../tests/unit/tools/localize_issue/localize_issue_test.rs"]
mod tests;
