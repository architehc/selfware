//! SWE-bench-specific memory / RAG with leakage controls.
//!
//! - `SwebenchInstanceMemory` tracks per-instance lessons across retries.
//! - `SwebenchMemoryStore` persists them on disk.
//! - Indexing helpers capture cheap repo snapshots (file tree, signatures, test
//!   locations) so the agent starts with context on the first turn.
//! - Leakage controls ensure exact-instance memory is NEVER reused for official
//!   pass@1 runs and repo-level lessons are strictly capped.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::token_count::estimate_content_tokens;

// ─── Constants ──────────────────────────────────────────────────────────────

/// Hard cap on repo-level lesson tokens injected into a single prompt.
pub const REPO_LESSON_TOKEN_CAP: usize = 2_000;

/// Hard cap on exact-instance lesson tokens injected into a single prompt.
pub const INSTANCE_LESSON_TOKEN_CAP: usize = 2_000;

/// Maximum total memory tokens allowed in a prompt (repo + instance).
pub const TOTAL_MEMORY_TOKEN_CAP: usize = 2_000;

// ─── Data Types ─────────────────────────────────────────────────────────────

/// Unique key for an instance memory entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryKey {
    pub repo: String,
    pub base_commit: String,
    pub instance_id: String,
    pub quant: String,
    pub trial: u32,
}

impl MemoryKey {
    pub fn new(repo: &str, base_commit: &str, instance_id: &str, quant: &str, trial: u32) -> Self {
        Self {
            repo: repo.to_string(),
            base_commit: base_commit.to_string(),
            instance_id: instance_id.to_string(),
            quant: quant.to_string(),
            trial,
        }
    }

    /// Returns the directory name used for on-disk storage.
    pub fn dir_name(&self) -> String {
        format!(
            "{}__{}__{}__trial{}",
            sanitize(&self.repo),
            &self.base_commit[..self.base_commit.len().min(12)],
            sanitize(&self.instance_id),
            self.trial
        )
    }
}

/// A single learned insight attached to a file location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lesson {
    pub file: String,
    pub line_range: Option<(usize, usize)>,
    pub insight: String,
    pub source: String, // "agent", "verification", "user"
    pub tokens: usize,
}

impl Lesson {
    pub fn new(file: &str, insight: &str, source: &str) -> Self {
        let tokens = estimate_content_tokens(insight);
        Self {
            file: file.to_string(),
            line_range: None,
            insight: insight.to_string(),
            source: source.to_string(),
            tokens,
        }
    }

    pub fn with_line_range(mut self, start: usize, end: usize) -> Self {
        self.line_range = Some((start, end));
        self
    }
}

/// Per-instance memory record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwebenchInstanceMemory {
    pub repo: String,
    pub base_commit: String,
    pub instance_id: String,
    pub quant: String,
    pub trial: u32,
    pub lessons: Vec<Lesson>,
    pub indexed_at: Option<DateTime<Utc>>,
}

impl SwebenchInstanceMemory {
    pub fn new(key: &MemoryKey) -> Self {
        Self {
            repo: key.repo.clone(),
            base_commit: key.base_commit.clone(),
            instance_id: key.instance_id.clone(),
            quant: key.quant.clone(),
            trial: key.trial,
            lessons: Vec::new(),
            indexed_at: None,
        }
    }

    pub fn total_lesson_tokens(&self) -> usize {
        self.lessons.iter().map(|l| l.tokens).sum()
    }

    pub fn add_lesson(&mut self, lesson: Lesson) {
        self.lessons.push(lesson);
    }
}

/// Cheap repo index captured before the first agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndex {
    pub repo: String,
    pub base_commit: String,
    pub indexed_at: DateTime<Utc>,
    pub file_tree: Vec<String>,
    pub key_signatures: Vec<FileSignature>,
    pub test_files: Vec<String>,
}

/// A single function / class / method signature extracted from the repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSignature {
    pub file: String,
    pub symbol: String,
    pub signature: String,
    pub line: usize,
}

// ─── Indexing helpers ───────────────────────────────────────────────────────

/// Build a cheap repo index for `workdir`.
pub fn index_repo(repo: &str, base_commit: &str, workdir: &Path) -> Result<RepoIndex> {
    let mut file_tree = Vec::new();
    let mut test_files = Vec::new();
    let mut key_signatures = Vec::new();

    for entry in walkdir::WalkDir::new(workdir)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let rel = path
            .strip_prefix(workdir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        // Skip binary / huge files.
        let meta = entry.metadata();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        if size > 512_000 {
            continue;
        }

        file_tree.push(rel.clone());

        let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_test =
            basename.contains("test") || rel.contains("/tests/") || rel.contains("/test/");
        if is_test {
            test_files.push(rel.clone());
        }

        // Extract signatures for known code files.
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "py" | "rs" | "js" | "ts" | "go" | "java" | "cpp" | "c" | "h"
            ) && size < 100_000
            {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let sigs = extract_signatures(&rel, &content);
                    key_signatures.extend(sigs);
                }
            }
        }
    }

    // Cap the number of signatures to keep the index small.
    key_signatures.truncate(500);

    Ok(RepoIndex {
        repo: repo.to_string(),
        base_commit: base_commit.to_string(),
        indexed_at: Utc::now(),
        file_tree,
        key_signatures,
        test_files,
    })
}

/// Write a `RepoIndex` to disk under `memory_dir`.
pub fn save_repo_index(index: &RepoIndex, memory_dir: &Path) -> Result<()> {
    let dir = memory_dir
        .join(sanitize(&index.repo))
        .join(&index.base_commit[..index.base_commit.len().min(12)]);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("index.json");
    std::fs::write(&path, serde_json::to_vec_pretty(index)?)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Load a `RepoIndex` from disk if it exists.
pub fn load_repo_index(repo: &str, base_commit: &str, memory_dir: &Path) -> Option<RepoIndex> {
    let dir = memory_dir
        .join(sanitize(repo))
        .join(&base_commit[..base_commit.len().min(12)]);
    let path = dir.join("index.json");
    std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

// ─── Prompt injection helpers ───────────────────────────────────────────────

/// Build a memory-injection block for a prompt, respecting all leakage controls.
///
/// * `official_mode` – when `true`, returns `None` (no memory injection).
/// * `trial` – when `1`, exact-instance memory is suppressed (only repo-level
///   lessons are allowed).
/// * `candidate_files` – files the agent is likely to touch; used to rank
///   lesson relevance.
///
/// Returns `Some(block)` when there is relevant memory to inject, `None`
/// otherwise.
pub fn build_memory_prompt_block(
    instance_memory: Option<&SwebenchInstanceMemory>,
    repo_lessons: &[Lesson],
    official_mode: bool,
    trial: u32,
    candidate_files: &[String],
) -> Option<String> {
    if official_mode {
        return None;
    }

    let candidate_set: HashSet<String> = candidate_files.iter().cloned().collect();

    let mut blocks: Vec<String> = Vec::new();
    let mut tokens_used: usize = 0;

    // ── Repo-level lessons (always allowed, capped) ──
    let mut repo_lessons_scored: Vec<(&Lesson, f32)> = repo_lessons
        .iter()
        .map(|l| {
            let mut score = 0.0f32;
            if candidate_set.contains(&l.file) {
                score += 2.0;
            }
            // Fuzzy path match.
            for cf in candidate_files {
                if l.file.contains(cf) || cf.contains(&l.file) {
                    score += 1.0;
                }
            }
            (l, score)
        })
        .collect();
    repo_lessons_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (lesson, _score) in repo_lessons_scored {
        if tokens_used + lesson.tokens > REPO_LESSON_TOKEN_CAP {
            break;
        }
        if lesson.tokens == 0 {
            continue;
        }
        blocks.push(format_lesson(lesson));
        tokens_used += lesson.tokens;
    }

    // ── Exact-instance lessons (retries only: trial > 1) ──
    if trial > 1 {
        if let Some(mem) = instance_memory {
            let mut inst_lessons_scored: Vec<(&Lesson, f32)> = mem
                .lessons
                .iter()
                .map(|l| {
                    let mut score = 0.0f32;
                    if candidate_set.contains(&l.file) {
                        score += 2.0;
                    }
                    for cf in candidate_files {
                        if l.file.contains(cf) || cf.contains(&l.file) {
                            score += 1.0;
                        }
                    }
                    (l, score)
                })
                .collect();
            inst_lessons_scored
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            for (lesson, _score) in inst_lessons_scored {
                if tokens_used + lesson.tokens > TOTAL_MEMORY_TOKEN_CAP {
                    break;
                }
                if lesson.tokens == 0 {
                    continue;
                }
                blocks.push(format_lesson(lesson));
                tokens_used += lesson.tokens;
            }
        }
    }

    if blocks.is_empty() {
        return None;
    }

    Some(format!(
        "[memory: {} tokens]\n{}
[end memory]\n\n",
        tokens_used,
        blocks.join("\n")
    ))
}

fn format_lesson(lesson: &Lesson) -> String {
    let loc = match lesson.line_range {
        Some((s, e)) => format!("{}:{}-{}", lesson.file, s, e),
        None => lesson.file.clone(),
    };
    format!(
        "- [{} | {}] {}\n  (this insight is for reference only — do NOT include it in patches)",
        loc, lesson.source, lesson.insight
    )
}

// ─── Internal helpers ───────────────────────────────────────────────────────

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn extract_signatures(file: &str, content: &str) -> Vec<FileSignature> {
    let mut out = Vec::new();
    for (line_num_0, line) in content.lines().enumerate() {
        let line_num = line_num_0 + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("#") || trimmed.starts_with("//") {
            continue;
        }

        // Python defs.
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            if let Some(name) = extract_name(trimmed, "def ") {
                out.push(FileSignature {
                    file: file.to_string(),
                    symbol: name.clone(),
                    signature: trimmed.to_string(),
                    line: line_num,
                });
            }
            continue;
        }

        // Python classes.
        if trimmed.starts_with("class ") {
            if let Some(name) = extract_name(trimmed, "class ") {
                out.push(FileSignature {
                    file: file.to_string(),
                    symbol: name.clone(),
                    signature: trimmed.to_string(),
                    line: line_num,
                });
            }
            continue;
        }

        // Rust / JS / TS / Go / Java / C++ functions.
        if is_fn_line_multi_lang(trimmed) {
            if let Some(name) = extract_fn_name_multi_lang(trimmed) {
                out.push(FileSignature {
                    file: file.to_string(),
                    symbol: name.clone(),
                    signature: trimmed.to_string(),
                    line: line_num,
                });
            }
            continue;
        }
    }
    out
}

fn extract_name(line: &str, keyword: &str) -> Option<String> {
    let after = line.strip_prefix("async ").unwrap_or(line);
    let after = after.strip_prefix(keyword).unwrap_or(after);
    let end = after
        .find(|c: char| c == '(' || c == ':' || c == '<' || c == ' ')
        .unwrap_or(after.len());
    let name = after[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn is_fn_line_multi_lang(line: &str) -> bool {
    let prefixes = [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "async fn ",
        "pub async fn ",
        "function ",
        "public ",
        "private ",
        "static ",
        "const ",
        "void ",
        "int ",
        "bool ",
        "string ",
        "func ",
    ];
    prefixes.iter().any(|p| line.starts_with(p))
}

fn extract_fn_name_multi_lang(line: &str) -> Option<String> {
    // Strip common prefixes.
    let mut rest = line;
    for prefix in &[
        "pub(crate) ",
        "pub ",
        "async ",
        "static ",
        "const ",
        "private ",
        "public ",
    ] {
        rest = rest.strip_prefix(prefix).unwrap_or(rest);
    }

    // Strip return type prefixes for C-like languages.
    for prefix in &["void ", "int ", "bool ", "string ", "func ", "function "] {
        rest = rest.strip_prefix(prefix).unwrap_or(rest);
    }

    // Now look for "fn " if Rust.
    if let Some(after_fn) = rest.strip_prefix("fn ") {
        let end = after_fn
            .find(|c: char| c == '(' || c == '<' || c == ' ')
            .unwrap_or(after_fn.len());
        let name = after_fn[..end].trim();
        return if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        };
    }

    // Generic fallback: first identifier before '('.
    if let Some(before_paren) = rest.split('(').next() {
        let name = before_paren.trim().split_whitespace().last().unwrap_or("");
        if name.is_empty() || name == "if" || name == "for" || name == "while" {
            None
        } else {
            Some(name.to_string())
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lesson_serde_roundtrip() {
        let lesson =
            Lesson::new("src/main.rs", "Check null pointer here", "agent").with_line_range(10, 20);
        let json = serde_json::to_string(&lesson).unwrap();
        let parsed: Lesson = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file, "src/main.rs");
        assert_eq!(parsed.insight, "Check null pointer here");
        assert_eq!(parsed.source, "agent");
        assert_eq!(parsed.line_range, Some((10, 20)));
    }

    #[test]
    fn test_instance_memory_serde_roundtrip() {
        let key = MemoryKey::new("django/django", "abc123", "inst-1", "qwen-7b", 2);
        let mut mem = SwebenchInstanceMemory::new(&key);
        mem.add_lesson(Lesson::new("auth.py", "Use authenticate()", "verification"));
        mem.indexed_at = Some(Utc::now());

        let json = serde_json::to_string(&mem).unwrap();
        let parsed: SwebenchInstanceMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.instance_id, "inst-1");
        assert_eq!(parsed.lessons.len(), 1);
        assert!(parsed.indexed_at.is_some());
    }

    #[test]
    fn test_memory_key_dir_name() {
        let key = MemoryKey::new("a/b", "deadbeef1234", "id-1", "qwen", 3);
        let dir = key.dir_name();
        assert!(dir.contains("a_b"));
        assert!(dir.contains("deadbeef1234"));
        assert!(dir.contains("id-1"));
        assert!(dir.contains("trial3"));
    }

    #[test]
    fn test_leakage_control_official_mode_returns_none() {
        let lesson = Lesson::new("f.py", "insight", "agent");
        let mem = SwebenchInstanceMemory {
            repo: "r".into(),
            base_commit: "c".into(),
            instance_id: "i".into(),
            quant: "q".into(),
            trial: 2,
            lessons: vec![lesson],
            indexed_at: None,
        };
        let block = build_memory_prompt_block(
            Some(&mem),
            &[],
            true, // official_mode
            2,
            &[],
        );
        assert!(
            block.is_none(),
            "official mode must disable memory injection"
        );
    }

    #[test]
    fn test_leakage_control_trial_one_no_instance_memory() {
        let lesson = Lesson::new("f.py", "exact instance insight", "agent");
        let mem = SwebenchInstanceMemory {
            repo: "r".into(),
            base_commit: "c".into(),
            instance_id: "i".into(),
            quant: "q".into(),
            trial: 1,
            lessons: vec![lesson.clone()],
            indexed_at: None,
        };
        let block = build_memory_prompt_block(Some(&mem), &[], false, 1, &["f.py".into()]);
        // Trial 1 should not include exact-instance memory.
        assert!(
            block.is_none() || !block.as_ref().unwrap().contains("exact instance insight"),
            "trial 1 must not inject exact-instance memory"
        );
    }

    #[test]
    fn test_repo_lessons_allowed_on_trial_one() {
        let repo_lesson = Lesson::new("f.py", "repo-level insight", "agent");
        let block = build_memory_prompt_block(None, &[repo_lesson], false, 1, &["f.py".into()]);
        assert!(block.is_some());
        assert!(block.unwrap().contains("repo-level insight"));
    }

    #[test]
    fn test_token_capping_repo_lessons() {
        let big_insight = "word ".repeat(500); // ~500 tokens
        let lesson1 = Lesson::new("a.py", &big_insight, "agent");
        let lesson2 = Lesson::new("b.py", &big_insight, "agent");
        let block = build_memory_prompt_block(
            None,
            &[lesson1, lesson2],
            false,
            1,
            &["a.py".into(), "b.py".into()],
        );
        let block = block.unwrap();
        let tokens = estimate_content_tokens(&block);
        assert!(
            tokens <= TOTAL_MEMORY_TOKEN_CAP + 100, // small slack for formatting
            "repo lessons should be capped: got {} tokens",
            tokens
        );
    }

    #[test]
    fn test_build_memory_prompt_block_format() {
        let lesson =
            Lesson::new("src/lib.rs", "Watch for overflow", "verification").with_line_range(42, 50);
        let block = build_memory_prompt_block(None, &[lesson], false, 1, &["src/lib.rs".into()]);
        let block = block.unwrap();
        assert!(block.starts_with("[memory:"));
        assert!(block.contains("src/lib.rs:42-50"));
        assert!(block.contains("verification"));
        assert!(block.contains("do NOT include it in patches"));
        assert!(block.contains("[end memory]"));
    }

    #[test]
    fn test_extract_signatures_python() {
        let code = "def foo(x):\n    pass\n\nclass Bar:\n    def baz(self):\n        pass\n";
        let sigs = extract_signatures("test.py", code);
        let names: Vec<_> = sigs.iter().map(|s| s.symbol.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"Bar"));
        assert!(names.contains(&"baz"));
    }

    #[test]
    fn test_extract_signatures_rust() {
        let code = "pub fn main() {}\nfn helper() -> i32 { 0 }\n";
        let sigs = extract_signatures("main.rs", code);
        let names: Vec<_> = sigs.iter().map(|s| s.symbol.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));
    }

    #[test]
    fn test_repo_index_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let index = RepoIndex {
            repo: "test/repo".into(),
            base_commit: "abc123def456".into(),
            indexed_at: Utc::now(),
            file_tree: vec!["src/main.rs".into(), "tests/test.rs".into()],
            key_signatures: vec![FileSignature {
                file: "src/main.rs".into(),
                symbol: "main".into(),
                signature: "fn main()".into(),
                line: 1,
            }],
            test_files: vec!["tests/test.rs".into()],
        };
        save_repo_index(&index, tmp.path()).unwrap();
        let loaded = load_repo_index("test/repo", "abc123def456", tmp.path());
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.file_tree.len(), 2);
        assert_eq!(loaded.key_signatures.len(), 1);
        assert_eq!(loaded.test_files.len(), 1);
    }

    #[test]
    fn test_repo_index_load_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_repo_index("nonexistent", "000", tmp.path());
        assert!(loaded.is_none());
    }
}
