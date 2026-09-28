//! Was a failing check already failing before the task touched anything?
//!
//! Failure mode (c24 live run, 0.9.3 integration): the workspace's declared
//! `[[test]]` targets had no files (`tests/` missing), so `cargo check
//! --all-targets` failed at manifest load BEFORE the task started. The model
//! made a correct comments-only edit; the completion gate then attributed the
//! failure to that edit ("`cargo check` failed at the current revision (after
//! your edit to src/agent/context.rs)") and refused every final answer until
//! MAX_ITERATIONS. Nothing the task was allowed to change could fix it.
//!
//! The fix is evidence, not a waiver:
//!
//! 1. At task start, before any tool runs, the working tree (tracked files
//!    plus untracked, non-ignored ones — including uncommitted changes and
//!    deletions) is written to a git TREE object through a temporary index
//!    ([`capture_task_start_tree`]). The user's index and working tree are
//!    never touched.
//! 2. When the completion gate is about to block on a failing check, the same
//!    check is re-run ONCE on a throwaway checkout of that tree
//!    ([`Agent::attribute_blocking_failures`]), bounded by
//!    [`BASELINE_TIMEOUT`] and cached per check for the rest of the task.
//! 3. The two runs' error lines are normalised (workspace path, numbers) and
//!    compared ([`attribute`]). Only a failure whose errors ALL already
//!    occur on the pre-task tree is [`Attribution::PreExisting`]: it does not
//!    block, and the run reports it as failing before the task too — never as
//!    a pass. New errors still block, and the refusal names them.
//! 4. When the pre-task tree cannot be checked (not a git repository, the
//!    re-run timed out, no comparable error lines), the gate keeps blocking
//!    but says it could not tell, and the same unchanged failure may block at
//!    most [`MAX_UNATTRIBUTED_FAILURE_BLOCKS`] times before the run ends as
//!    an honest VERIFICATION_FAILED instead of burning the iteration budget.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::Agent;

/// Upper bound on one pre-task re-run of a check.
pub(crate) const BASELINE_TIMEOUT: Duration = Duration::from_secs(240);

/// How many times the SAME unchanged failure may block completion when it
/// could not be established whether it pre-existed the task.
pub(crate) const MAX_UNATTRIBUTED_FAILURE_BLOCKS: usize = 3;

/// Marker in the run-ending error when [`MAX_UNATTRIBUTED_FAILURE_BLOCKS`] is
/// reached. Terminal (never auto-recovered) and classified
/// VERIFICATION_FAILED.
pub(crate) const UNATTRIBUTED_FAILURE_LOOP_MARKER: &str = "UNATTRIBUTED_FAILURE_LOOP";

/// Error lines kept per record (raw) — bounds checkpoint size.
const MAX_DIAGNOSTIC_LINES: usize = 200;

/// Longest single diagnostic line kept. Generous on purpose: lines carry
/// absolute paths (a temp checkout's is ~120 characters), and a path cut in
/// half can no longer be normalised to `<root>`.
const MAX_DIAGNOSTIC_CHARS: usize = 1_000;

/// Untracked files larger than this (in total) are left out of the task-start
/// snapshot: hashing them into the object database costs more than the
/// comparison is worth. The comparison is then flagged as approximate.
const MAX_UNTRACKED_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNTRACKED_SNAPSHOT_FILES: usize = 5_000;

/// How to run a recorded check again, on another tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RerunSpec {
    /// A verification tool the model called (`cargo_check`, `shell_exec`
    /// with `cargo test` / `pytest` / `npm test` / `go test`, ...), with the
    /// exact arguments it was called with (JSON text).
    Tool { name: String, args: String },
    /// A post-edit verification-gate check (`type_check`, `test`, `lint`, the
    /// language-QA stages) triggered by edits to `paths`.
    PostEditGate {
        paths: Vec<String>,
        check_type: String,
    },
}

/// Whether a failing check's errors already existed before the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Attribution {
    /// Every error the check reports now is also reported on the pre-task
    /// tree. Not caused by the task's change; does not block.
    PreExisting { sample: Vec<String> },
    /// The check reports errors the pre-task tree does not have (or the
    /// pre-task tree passes it). Caused by the task's change; blocks.
    New { new_errors: Vec<String> },
    /// The pre-task tree fails this check too, but with entirely different
    /// errors — nothing to attribute either way. Blocks, capped.
    Different {
        before: Vec<String>,
        now: Vec<String>,
    },
    /// Could not be determined (no pre-task snapshot, the re-run timed out
    /// or could not start, no comparable error lines). Blocks, capped.
    Unknown { reason: String },
}

impl Attribution {
    pub fn is_preexisting(&self) -> bool {
        matches!(self, Attribution::PreExisting { .. })
    }

    /// Whether blocks on this failure count toward
    /// [`MAX_UNATTRIBUTED_FAILURE_BLOCKS`]: only when the task's change could
    /// not be shown to have caused it.
    pub fn block_is_capped(&self) -> bool {
        matches!(
            self,
            Attribution::Different { .. } | Attribution::Unknown { .. }
        )
    }

    /// The sentence appended to a completion-gate refusal.
    pub fn gate_note(&self) -> String {
        match self {
            Attribution::PreExisting { .. } => String::new(),
            Attribution::New { new_errors } => format!(
                " This is caused by the task's changes: the tree the task started from does not \
                 report {}: {}.",
                if new_errors.len() == 1 {
                    "this error"
                } else {
                    "these errors"
                },
                join_sample(new_errors)
            ),
            Attribution::Different { before, .. } => format!(
                " The tree the task started from fails this check too, but with different errors \
                 (before the task: {}), so selfware cannot tell whether your change caused the \
                 current ones.",
                join_sample(before)
            ),
            Attribution::Unknown { reason } => format!(
                " Selfware could not tell whether this failure already existed before the task \
                 ({reason})."
            ),
        }
    }
}

fn join_sample(lines: &[String]) -> String {
    let shown: Vec<String> = lines.iter().take(3).map(|l| format!("`{l}`")).collect();
    let more = lines.len().saturating_sub(shown.len());
    if more > 0 {
        format!("{} (+{more} more)", shown.join("; "))
    } else {
        shown.join("; ")
    }
}

/// A human line for the run summary / verdict describing a pre-existing
/// failure: `` `cargo check`: failing before the task too (pre-existing:
/// …) — not caused by this change ``.
pub(crate) fn preexisting_note(check_id: &str, attribution: &Attribution) -> String {
    let check = check_id.strip_prefix("gate:").unwrap_or(check_id);
    let sample = match attribution {
        Attribution::PreExisting { sample } => sample.first().cloned().unwrap_or_default(),
        _ => String::new(),
    };
    let sample: String = sample.chars().take(80).collect();
    if sample.is_empty() {
        format!("`{check}`: failing before the task too (pre-existing) — not caused by this change")
    } else {
        format!(
            "`{check}`: failing before the task too (pre-existing: {sample}…) — not caused by this change"
        )
    }
}

// ---------------------------------------------------------------------------
// Diagnostics: which error lines a check's output carries.
// ---------------------------------------------------------------------------

/// Error-shaped lines of a check's output, raw (un-normalised), in order
/// and bounded. Accepts a tool's JSON result (cargo tools: structured
/// `errors`; shell: `stdout`/`stderr`) or plain text. Warnings are never
/// diagnostics: they do not fail a check.
pub(crate) fn diagnostic_lines(output: &str) -> Vec<String> {
    let mut lines = Vec::new();
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(value) if value.is_object() || value.is_array() => {
            let mut text = String::new();
            collect_json(&value, &mut lines, &mut text);
            scan_text(&text, &mut lines);
        }
        _ => scan_text(output, &mut lines),
    }
    // NOT deduplicated: the comparison counts occurrences, so a second
    // `mismatched types` in the same file (line numbers are normalised away)
    // is a new error, not the old one again.
    lines.truncate(MAX_DIAGNOSTIC_LINES);
    lines
}

/// Error lines of a post-edit gate check: its output text plus its
/// structured error-severity diagnostics.
pub(crate) fn check_result_diagnostic_lines(
    check: &crate::testing::verification::CheckResult,
) -> Vec<String> {
    let mut text = check.output.clone();
    for error in &check.errors {
        if matches!(
            error.severity,
            crate::testing::verification::ErrorSeverity::Error
        ) {
            text.push('\n');
            text.push_str(&structured_error_line(
                error.code.as_deref(),
                &error.message,
                Some(&error.file),
            ));
        }
    }
    diagnostic_lines(&text)
}

fn structured_error_line(code: Option<&str>, message: &str, file: Option<&str>) -> String {
    let first = message.lines().next().unwrap_or_default().trim();
    let mut line = match code {
        Some(code) if !code.is_empty() => format!("error[{code}]: {first}"),
        _ => format!("error: {first}"),
    };
    if let Some(file) = file.filter(|f| !f.is_empty()) {
        line.push_str(" @ ");
        line.push_str(file);
    }
    line
}

/// JSON keys that only repeat (or add non-failing) content.
const SKIPPED_JSON_KEYS: &[&str] = &[
    "warnings",
    "by_file",
    "first_error",
    "analysis",
    "snippet",
    "suggestion",
    "suggestions",
    "by_category",
];

fn collect_json(value: &serde_json::Value, lines: &mut Vec<String>, text: &mut String) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let str_field = |k: &str| map.get(k).and_then(Value::as_str);
            // A structured compiler/lint diagnostic.
            if let Some(message) = str_field("message") {
                let is_diag = ["file", "code", "severity", "level", "line"]
                    .iter()
                    .any(|k| map.contains_key(*k));
                if is_diag {
                    let level = str_field("severity")
                        .or_else(|| str_field("level"))
                        .unwrap_or("error")
                        .to_ascii_lowercase();
                    if matches!(level.as_str(), "error" | "deny" | "forbid" | "fatal") {
                        lines.push(structured_error_line(
                            str_field("code"),
                            message,
                            str_field("file"),
                        ));
                    }
                    return;
                }
            }
            // A failed test (cargo_test `failures[]`, `tests[]`).
            if let Some(test) = str_field("test_name") {
                lines.push(format!("FAILED test {test}"));
                return;
            }
            if let (Some(name), Some("failed")) = (str_field("name"), str_field("status")) {
                lines.push(format!("FAILED test {name}"));
                return;
            }
            for (key, child) in map {
                if SKIPPED_JSON_KEYS.contains(&key.as_str()) {
                    continue;
                }
                collect_json(child, lines, text);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_json(item, lines, text);
            }
        }
        Value::String(s) => {
            text.push_str(s);
            text.push('\n');
        }
        _ => {}
    }
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn is_exception_line(line: &str) -> bool {
    // `SyntaxError: …`, `ModuleNotFoundError: …`, `AssertionError [ERR_…]`.
    let head: String = line
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    (head.ends_with("Error") || head.ends_with("Exception"))
        && line[head.len()..].starts_with([':', ' '])
        && head.len() < line.len()
}

fn scan_text(text: &str, lines: &mut Vec<String>) {
    let raw: Vec<String> = text
        .lines()
        .map(|l| strip_ansi(l).trim().to_string())
        .collect();
    let mut i = 0;
    while i < raw.len() {
        let line = &raw[i];
        let lower = line.to_ascii_lowercase();
        let keep = if line.starts_with("error:") || line.starts_with("error[") {
            // rustc/cargo: attach the ` --> file:line:col` location that
            // follows, so the same message in two files stays two errors.
            let location = raw
                .get(i + 1)
                .and_then(|next| next.strip_prefix("--> "))
                .map(str::to_string);
            Some(match location {
                Some(loc) => {
                    i += 1;
                    format!("{line} @ {loc}")
                }
                None => line.clone(),
            })
        } else if line.starts_with("FAILED ") || line.starts_with("ERROR ") {
            // pytest short summary: keep the node id, not the reason text.
            Some(line.split(" - ").next().unwrap_or(line).to_string())
        } else if line.starts_with("--- FAIL:")
            || (line.starts_with("FAIL") && line.len() > 5)
            || line.starts_with("not ok ")
            || line.starts_with("● ")
            || line.ends_with("... FAILED")
            || line.ends_with("... FAIL")
            || lower.contains(": error")
            || lower.contains("): error")
            || is_exception_line(line)
        {
            Some(line.clone())
        } else {
            None
        };
        if let Some(keep) = keep {
            lines.push(keep.chars().take(MAX_DIAGNOSTIC_CHARS).collect());
        }
        i += 1;
    }
}

/// Normalise one diagnostic line for comparison across trees: the tree's
/// root path(s) become `<root>`, every digit run becomes `#` (line/column
/// numbers shift under a harmless edit; counts in "due to N previous
/// errors" change when an error is fixed), whitespace collapses.
pub(crate) fn normalize_diagnostic(line: &str, roots: &[String]) -> String {
    let mut s = line.to_string();
    for root in roots {
        if !root.is_empty() {
            s = s.replace(root.as_str(), "<root>");
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut in_digits = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            in_digits = false;
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Root-path spellings to replace, longest first (raw and canonical: macOS
/// `/tmp` ↔ `/private/tmp`, `/var` ↔ `/private/var`).
pub(crate) fn root_spellings(roots: &[&Path]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for root in roots {
        out.push(root.to_string_lossy().into_owned());
        if let Ok(canon) = std::fs::canonicalize(root) {
            out.push(canon.to_string_lossy().into_owned());
        }
    }
    out.retain(|s| !s.is_empty() && s != "/");
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out.dedup();
    out
}

/// Normalised error lines with their occurrence counts.
pub(crate) type DiagnosticCounts = BTreeMap<String, usize>;

pub(crate) fn normalized_counts(lines: &[String], roots: &[String]) -> DiagnosticCounts {
    let mut counts = DiagnosticCounts::new();
    for line in lines {
        let norm = normalize_diagnostic(line, roots);
        if !norm.is_empty() {
            *counts.entry(norm).or_default() += 1;
        }
    }
    counts
}

/// The outcome of running a check on the pre-task tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BaselineRun {
    /// The check ran: whether it passed, and its normalised error lines.
    Ran {
        passed: bool,
        diagnostics: DiagnosticCounts,
    },
    /// The check could not be run on the pre-task tree; why.
    Unavailable(String),
}

/// Compare a failing check's normalised errors with the same check on the
/// pre-task tree. An error is new when it occurs MORE often now than before
/// (occurrences, not just distinct lines: the same message in the same file
/// is indistinguishable once line numbers are normalised away).
pub(crate) fn attribute(current: &DiagnosticCounts, baseline: &BaselineRun) -> Attribution {
    let sample = |keys: Vec<&String>| keys.into_iter().take(3).cloned().collect::<Vec<_>>();
    let (passed, before) = match baseline {
        BaselineRun::Unavailable(reason) => {
            return Attribution::Unknown {
                reason: reason.clone(),
            }
        }
        BaselineRun::Ran {
            passed,
            diagnostics,
        } => (*passed, diagnostics),
    };
    if current.is_empty() {
        return Attribution::Unknown {
            reason: "the failing output has no recognisable error lines to compare".to_string(),
        };
    }
    if passed {
        return Attribution::New {
            new_errors: sample(current.keys().collect()),
        };
    }
    if before.is_empty() {
        return Attribution::Unknown {
            reason: "the check fails on the pre-task tree too, but its output there has no \
                     recognisable error lines to compare"
                .to_string(),
        };
    }
    let new: Vec<&String> = current
        .iter()
        .filter(|(line, n)| before.get(*line).copied().unwrap_or(0) < **n)
        .map(|(line, _)| line)
        .collect();
    let shared = current.keys().any(|line| before.contains_key(line));
    if new.is_empty() {
        Attribution::PreExisting {
            sample: sample(current.keys().collect()),
        }
    } else if shared {
        Attribution::New {
            new_errors: sample(new),
        }
    } else {
        Attribution::Different {
            before: sample(before.keys().collect()),
            now: sample(current.keys().collect()),
        }
    }
}

// ---------------------------------------------------------------------------
// The pre-task tree: capture (task start) and checkout (on demand).
// ---------------------------------------------------------------------------

fn git(dir: &Path, index: Option<&Path>, args: &[&str]) -> Option<std::process::Output> {
    let mut cmd =
        crate::safety::git_exec::git_command(dir, crate::safety::git_exec::GitScope::Internal);
    cmd.args(args);
    if let Some(index) = index {
        cmd.env("GIT_INDEX_FILE", index);
    }
    cmd.output().ok()
}

fn git_stdout(dir: &Path, index: Option<&Path>, args: &[&str]) -> Option<String> {
    let out = git(dir, index, args)?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The repository top level containing `dir`, if any.
pub(crate) fn repo_toplevel(dir: &Path) -> Option<PathBuf> {
    git_stdout(dir, None, &["rev-parse", "--show-toplevel"])
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

struct TempIndex(PathBuf);

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("lock"));
    }
}

/// Write the working tree containing `dir` — tracked files as they are on
/// disk (uncommitted edits and deletions included) plus untracked,
/// non-ignored files — to a git tree object, and return its id.
///
/// Runs against a COPY of the index (`GIT_INDEX_FILE`), so the user's index,
/// staging state and working tree are untouched; the only side effect is
/// blob/tree objects in the object database (unreferenced, collected by `git
/// gc`). `None` outside a git repository or when any step fails.
///
/// Must run before the task's first tool call: a snapshot taken after an
/// edit would make that edit's errors look pre-existing.
pub(crate) fn capture_task_start_tree(dir: &Path) -> Option<String> {
    let top = repo_toplevel(dir)?;
    let index = TempIndex(std::env::temp_dir().join(format!(
        "selfware-start-index-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    )));
    // Seed from the real index so unchanged files are not re-hashed.
    if let Some(real) = git_stdout(&top, None, &["rev-parse", "--git-path", "index"]) {
        let real = if Path::new(&real).is_absolute() {
            PathBuf::from(real)
        } else {
            top.join(real)
        };
        if real.is_file() {
            std::fs::copy(&real, &index.0).ok()?;
        }
    }
    let untracked = git(
        &top,
        Some(&index.0),
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?;
    let (mut count, mut bytes) = (0usize, 0u64);
    for rel in untracked
        .stdout
        .split(|&b| b == 0)
        .filter(|c| !c.is_empty())
    {
        count += 1;
        bytes += std::fs::metadata(top.join(String::from_utf8_lossy(rel).as_ref()))
            .map(|m| m.len())
            .unwrap_or(0);
    }
    let include_untracked =
        count <= MAX_UNTRACKED_SNAPSHOT_FILES && bytes <= MAX_UNTRACKED_SNAPSHOT_BYTES;
    if !include_untracked {
        debug!(
            "task-start snapshot: {count} untracked files ({bytes} bytes) left out; \
             pre-existing-failure comparisons may see extra baseline errors"
        );
    }
    let add = if include_untracked { "-A" } else { "-u" };
    let added = git(&top, Some(&index.0), &["add", add, "--", "."])?;
    if !added.status.success() {
        debug!(
            "task-start snapshot: git add failed: {}",
            String::from_utf8_lossy(&added.stderr)
        );
        return None;
    }
    git_stdout(&top, Some(&index.0), &["write-tree"]).filter(|t| !t.is_empty())
}

/// A throwaway checkout of the task-start tree. Removed on drop.
#[derive(Debug)]
pub(crate) struct BaselineCheckout {
    /// Holds the checkout plus the `.cargo/config.toml` that routes its build
    /// output (outside the checked-out tree, so the tree stays byte-exact).
    base: PathBuf,
    /// The checked-out tree: the repository top level at task start.
    tree: PathBuf,
    /// The live repository top level the tree mirrors.
    toplevel: PathBuf,
}

impl Drop for BaselineCheckout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl BaselineCheckout {
    /// Check `tree_id` out of the repository at `toplevel` into a fresh
    /// temporary directory (a temporary index; no worktree registration).
    pub(crate) fn materialize(toplevel: &Path, tree_id: &str) -> Result<Self, String> {
        let base = std::env::temp_dir().join(format!(
            "selfware-baseline-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let tree = base.join("tree");
        std::fs::create_dir_all(&tree).map_err(|e| format!("cannot create checkout dir: {e}"))?;
        let checkout = Self {
            base: base.clone(),
            tree: tree.clone(),
            toplevel: toplevel.to_path_buf(),
        };
        let index = TempIndex(base.join("index"));
        let read = git(toplevel, Some(&index.0), &["read-tree", tree_id])
            .ok_or("git could not be started")?;
        if !read.status.success() {
            return Err(format!(
                "the task-start tree {tree_id} is no longer readable: {}",
                String::from_utf8_lossy(&read.stderr).trim()
            ));
        }
        let prefix = format!("{}/", tree.to_string_lossy());
        let out = git(
            toplevel,
            Some(&index.0),
            &["checkout-index", "-a", "-f", &format!("--prefix={prefix}")],
        )
        .ok_or("git could not be started")?;
        if !out.status.success() {
            return Err(format!(
                "checking out the task-start tree failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(checkout)
    }

    /// The checkout's equivalent of a live path under the repository.
    pub(crate) fn map(&self, live: &Path) -> Option<PathBuf> {
        let rel = strip_root(live, &self.toplevel)?;
        Some(self.tree.join(rel))
    }

    pub(crate) fn tree(&self) -> &Path {
        &self.tree
    }

    pub(crate) fn toplevel(&self) -> &Path {
        &self.toplevel
    }

    /// Route cargo's build output for the next re-run. `cargo check` /
    /// `clippy` share the live project's target directory (dependencies are
    /// reused; the checkout's own crates get distinct unit hashes because
    /// their path differs, and check/clippy uplift no binaries). Anything
    /// that links or runs (`cargo test`, `build`) gets an isolated directory
    /// under it, so the live `target/debug/<bin>` is never replaced by a
    /// pre-task build. The config sits ABOVE the tree, so a project's own
    /// `.cargo/config.toml` still wins, exactly as in the live tree.
    fn route_cargo_output(&self, live_target: Option<&Path>, check_only: bool) {
        let config_dir = self.base.join(".cargo");
        let Some(target) = live_target else {
            let _ = std::fs::remove_file(config_dir.join("config.toml"));
            return;
        };
        let target = if check_only {
            target.to_path_buf()
        } else {
            target.join("selfware-baseline")
        };
        let escaped = target
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let _ = std::fs::create_dir_all(&config_dir);
        let _ = std::fs::write(
            config_dir.join("config.toml"),
            format!("[build]\ntarget-dir = \"{escaped}\"\n"),
        );
    }
}

fn strip_root(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(root) {
        return Some(rel.to_path_buf());
    }
    // Spellings differ (macOS `/var` vs `/private/var`, symlinked roots):
    // compare canonical forms, canonicalising the deepest EXISTING ancestor
    // of `path` (it may name a file that does not exist yet).
    let canon_root = std::fs::canonicalize(root).ok()?;
    let mut existing = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let canon = loop {
        if let Ok(c) = std::fs::canonicalize(&existing) {
            break c;
        }
        rest.push(existing.file_name()?.to_os_string());
        existing = existing.parent()?.to_path_buf();
    };
    let mut full = canon;
    for part in rest.into_iter().rev() {
        full.push(part);
    }
    full.strip_prefix(&canon_root).ok().map(Path::to_path_buf)
}

/// Replace every spelling of `from` with `to` in each string of a JSON value.
fn rebase_json_strings(value: &mut serde_json::Value, from: &[String], to: &str) {
    use serde_json::Value;
    match value {
        Value::String(s) => {
            for f in from {
                if s.contains(f.as_str()) {
                    *s = s.replace(f.as_str(), to);
                }
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|v| rebase_json_strings(v, from, to)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|v| rebase_json_strings(v, from, to)),
        _ => {}
    }
}

/// The live cargo target directory for a project: the nearest ancestor
/// (within the repository) whose manifest declares `[workspace]`, else the
/// nearest manifest. A wrong guess only costs cache reuse, never
/// correctness.
fn live_cargo_target(project: &Path, toplevel: &Path) -> Option<PathBuf> {
    let nearest = super::verification_scope::cargo_project_root(project)?;
    let mut dir = Some(nearest.as_path());
    while let Some(d) = dir {
        if !d.starts_with(toplevel) {
            break;
        }
        if std::fs::read_to_string(d.join("Cargo.toml"))
            .is_ok_and(|m| m.lines().any(|l| l.trim() == "[workspace]"))
        {
            return Some(d.join("target"));
        }
        dir = d.parent();
    }
    Some(nearest.join("target"))
}

fn rerun_is_check_only(spec: &RerunSpec) -> bool {
    match spec {
        RerunSpec::Tool { name, args } => {
            matches!(name.as_str(), "cargo_check" | "cargo_clippy")
                || serde_json::from_str::<serde_json::Value>(args)
                    .ok()
                    .and_then(|v| {
                        v.get("command")
                            .and_then(|c| c.as_str())
                            .map(str::to_string)
                    })
                    .is_some_and(|c| {
                        let id = super::verification_scope::check_id_for(name, &c);
                        id.starts_with("cargo check") || id.starts_with("cargo clippy")
                    })
        }
        RerunSpec::PostEditGate { check_type, .. } => {
            matches!(check_type.as_str(), "type_check" | "lint")
        }
    }
}

/// Per-task state: the checkout (created on first need) and the baseline
/// result of each distinct check, plus the unattributed-block tally.
#[derive(Debug, Default)]
pub(crate) struct BaselineState {
    checkout: Option<Result<Arc<BaselineCheckout>, String>>,
    runs: HashMap<String, BaselineRun>,
    blocks: Option<BlockTally>,
}

#[derive(Debug, Clone)]
struct BlockTally {
    fingerprint: String,
    count: usize,
    last_iteration: usize,
}

fn record_fingerprint(record: &super::verification_scope::VerificationRecord) -> String {
    let body = if record.diagnostics.is_empty() {
        normalize_diagnostic(&record.summary, &[])
    } else {
        let mut lines: Vec<String> = record
            .diagnostics
            .iter()
            .map(|l| normalize_diagnostic(l, &[]))
            .collect();
        lines.sort();
        lines.join("\n")
    };
    format!("{}\u{1f}{body}", record.check_id)
}

impl Agent {
    /// Decide, for every failure that would block completion and has not been
    /// judged yet, whether it already existed before the task (see the module
    /// docs). Cheap when there is nothing to judge; each distinct check is
    /// re-run on the pre-task tree at most once per task.
    pub(super) async fn attribute_blocking_failures(&mut self) {
        let task_root = self.verification_task_root();
        loop {
            let sequence = self.mutation_sequence;
            let Some(index) = self
                .verification_failures
                .outstanding()
                .iter()
                .position(|r| r.attribution.is_none() && r.blocks_completion(&task_root, sequence))
            else {
                break;
            };
            let record = self.verification_failures.outstanding()[index].clone();
            let attribution = self.attribute_failure(&record).await;
            info!(
                "verification failure `{}` at mutation #{}: {:?}",
                record.check_id, record.mutation_sequence, attribution
            );
            self.verification_failures
                .set_attribution(index, attribution);
        }
        // Keep the gate's summary in step with the ledger: a pre-existing
        // failure is no longer the "latest unresolved failure".
        self.last_failed_verification_summary = self
            .verification_failures
            .blocking(&task_root, self.mutation_sequence)
            .map(|failed| failed.summary.clone());
    }

    async fn attribute_failure(
        &mut self,
        record: &super::verification_scope::VerificationRecord,
    ) -> Attribution {
        let Some(spec) = record.rerun.clone() else {
            return Attribution::Unknown {
                reason: "this check cannot be re-run on the pre-task tree".to_string(),
            };
        };
        if record.diagnostics.is_empty() {
            return Attribution::Unknown {
                reason: "the failing output has no recognisable error lines to compare".to_string(),
            };
        }
        let key = serde_json::to_string(&spec).unwrap_or_default();
        let baseline = match self.baseline_state.runs.get(&key) {
            Some(run) => run.clone(),
            None => {
                let run = match tokio::time::timeout(
                    BASELINE_TIMEOUT,
                    Box::pin(self.run_on_task_start_tree(record, &spec)),
                )
                .await
                {
                    Ok(run) => run,
                    Err(_) => BaselineRun::Unavailable(format!(
                        "re-running it on the pre-task tree exceeded {}s",
                        BASELINE_TIMEOUT.as_secs()
                    )),
                };
                self.baseline_state.runs.insert(key, run.clone());
                run
            }
        };
        let toplevel = self
            .baseline_state
            .checkout
            .as_ref()
            .and_then(|c| c.as_ref().ok())
            .map(|c| c.toplevel().to_path_buf())
            .or_else(|| repo_toplevel(&self.verification_task_root()));
        let roots = match &toplevel {
            Some(top) => root_spellings(&[top.as_path()]),
            None => root_spellings(&[self.verification_task_root().as_path()]),
        };
        attribute(&normalized_counts(&record.diagnostics, &roots), &baseline)
    }

    async fn baseline_checkout(&mut self) -> Result<Arc<BaselineCheckout>, String> {
        if let Some(existing) = &self.baseline_state.checkout {
            return existing.clone();
        }
        let tree = self
            .current_checkpoint
            .as_ref()
            .and_then(|cp| cp.task_start_tree.clone());
        let task_root = self.verification_task_root();
        // git runs synchronously: keep it off the async worker thread.
        let result = tokio::task::spawn_blocking(move || {
            let tree = tree.ok_or_else(|| {
                "no snapshot of the pre-task tree was taken (not a git repository, or the \
                 task was resumed from an older checkpoint)"
                    .to_string()
            })?;
            let toplevel = repo_toplevel(&task_root)
                .ok_or_else(|| "the workspace is no longer a git repository".to_string())?;
            BaselineCheckout::materialize(&toplevel, &tree).map(Arc::new)
        })
        .await
        .unwrap_or_else(|e| Err(format!("checking out the pre-task tree panicked: {e}")));
        if let Err(reason) = &result {
            warn!("pre-task baseline unavailable: {reason}");
        }
        self.baseline_state.checkout = Some(result.clone());
        result
    }

    /// Run `spec` against the checkout of the task-start tree and return its
    /// outcome with normalised error lines.
    async fn run_on_task_start_tree(
        &mut self,
        record: &super::verification_scope::VerificationRecord,
        spec: &RerunSpec,
    ) -> BaselineRun {
        let checkout = match self.baseline_checkout().await {
            Ok(c) => c,
            Err(reason) => return BaselineRun::Unavailable(reason),
        };
        let live_roots = root_spellings(&[checkout.toplevel()]);
        let tree_str = checkout.tree().to_string_lossy().into_owned();
        let project = record
            .scope
            .project_root
            .clone()
            .unwrap_or_else(|| record.scope.working_dir.clone());
        if checkout.map(&record.scope.working_dir).is_none() {
            return BaselineRun::Unavailable(
                "the check ran outside the repository the pre-task snapshot covers".to_string(),
            );
        }
        checkout.route_cargo_output(
            live_cargo_target(&project, checkout.toplevel()).as_deref(),
            rerun_is_check_only(spec),
        );
        let baseline_roots = root_spellings(&[checkout.tree()]);
        let (passed, lines) = match spec {
            RerunSpec::Tool { name, args } => {
                let mut value = serde_json::from_str::<serde_json::Value>(args)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                rebase_json_strings(&mut value, &live_roots, &tree_str);
                let live_workspace = self.tools.workspace_root().path();
                let Some(workspace) = checkout.map(&live_workspace) else {
                    return BaselineRun::Unavailable(
                        "the workspace root lies outside the repository".to_string(),
                    );
                };
                let Some(tool) = self.tools.get(name) else {
                    return BaselineRun::Unavailable(format!("tool `{name}` is not available"));
                };
                let root = crate::tools::workspace_root::WorkspaceRoot::fixed(workspace);
                match crate::tools::workspace_root::scope(root, tool.execute(value)).await {
                    Ok(mut result) => {
                        let args_value = serde_json::from_str::<serde_json::Value>(args)
                            .unwrap_or(serde_json::Value::Null);
                        super::tool_dispatch::annotate_zero_test_verification(
                            name,
                            &args_value,
                            &mut result,
                        );
                        let passed =
                            super::tool_dispatch::tool_result_value_indicates_success(&result);
                        (passed, diagnostic_lines(&result.to_string()))
                    }
                    Err(e) => (false, diagnostic_lines(&e.to_string())),
                }
            }
            RerunSpec::PostEditGate { paths, check_type } => {
                let Some(gate_root) = checkout.map(self.verification_gate.project_root()) else {
                    return BaselineRun::Unavailable(
                        "the verification gate's project lies outside the repository".to_string(),
                    );
                };
                let working = self
                    .verification_gate
                    .working_dir()
                    .and_then(|w| checkout.map(w));
                let mut gate = self.verification_gate.rebased(gate_root, working);
                let paths: Vec<String> = paths
                    .iter()
                    .map(|p| {
                        let mut v = serde_json::Value::String(p.clone());
                        rebase_json_strings(&mut v, &live_roots, &tree_str);
                        v.as_str().unwrap_or(p).to_string()
                    })
                    .collect();
                let report = match gate.verify_change(&paths, "pre-task baseline").await {
                    Ok(report) => report,
                    Err(e) => {
                        return BaselineRun::Unavailable(format!(
                            "the post-edit checks could not run on the pre-task tree: {e}"
                        ))
                    }
                };
                let ran: Vec<_> = report
                    .checks
                    .iter()
                    .filter(|c| !c.not_run && c.check_type.as_str() == check_type)
                    .collect();
                if ran.is_empty() {
                    return BaselineRun::Unavailable(format!(
                        "`{check_type}` did not run on the pre-task tree"
                    ));
                }
                let passed = ran.iter().all(|c| c.passed);
                let lines = ran
                    .iter()
                    .filter(|c| !c.passed)
                    .flat_map(|c| check_result_diagnostic_lines(c))
                    .collect();
                (passed, lines)
            }
        };
        BaselineRun::Ran {
            passed,
            diagnostics: normalized_counts(&lines, &baseline_roots),
        }
    }

    /// Count a completion refusal against the unattributed-failure cap.
    ///
    /// Called where a refusal is actually delivered to the model. When the
    /// blocking failure could not be attributed ([`Attribution::block_is_capped`])
    /// and it is the SAME failure (same check, same error lines) that blocked
    /// before, the tally grows once per iteration; at
    /// [`MAX_UNATTRIBUTED_FAILURE_BLOCKS`] the run ends with
    /// [`UNATTRIBUTED_FAILURE_LOOP_MARKER`] (VERIFICATION_FAILED) instead of
    /// refusing until MAX_ITERATIONS. A failure the task's change caused
    /// (`New`) is never capped — the model can fix it.
    pub(super) fn note_unattributed_failure_block(&mut self) -> anyhow::Result<()> {
        let task_root = self.verification_task_root();
        let Some(record) = self
            .verification_failures
            .blocking(&task_root, self.mutation_sequence)
            .cloned()
        else {
            return Ok(());
        };
        if !record
            .attribution
            .as_ref()
            .is_some_and(Attribution::block_is_capped)
        {
            self.baseline_state.blocks = None;
            return Ok(());
        }
        let fingerprint = record_fingerprint(&record);
        let iteration = self.loop_control.current_iteration();
        let tally = match self.baseline_state.blocks.take() {
            Some(t) if t.fingerprint == fingerprint => {
                if t.last_iteration == iteration {
                    t
                } else {
                    BlockTally {
                        count: t.count + 1,
                        last_iteration: iteration,
                        fingerprint,
                    }
                }
            }
            _ => BlockTally {
                fingerprint,
                count: 1,
                last_iteration: iteration,
            },
        };
        let count = tally.count;
        self.baseline_state.blocks = Some(tally);
        if count >= MAX_UNATTRIBUTED_FAILURE_BLOCKS {
            let why = record
                .attribution
                .as_ref()
                .map(Attribution::gate_note)
                .unwrap_or_default();
            anyhow::bail!(
                "{UNATTRIBUTED_FAILURE_LOOP_MARKER}: `{}` kept failing unchanged and blocked \
                 completion {count} times.{why} Ending the run as failed instead of refusing \
                 until the iteration cap.",
                record.check_id
            );
        }
        Ok(())
    }

    /// True when a check ran on the CURRENT revision and every failure it
    /// left is shown to pre-exist the task, with nothing else blocking. The
    /// completion gates treat this like the not-runnable waiver: the demand
    /// for a passing check is dropped (no edit the task may make can turn it
    /// green), but NO verification credit is recorded — the run reports the
    /// failure as pre-existing, never as passed.
    pub(super) fn only_preexisting_failures_at_current_revision(&self) -> bool {
        let task_root = self.verification_task_root();
        self.mutation_sequence > 0
            && !self
                .verification_failures
                .preexisting(&task_root, self.mutation_sequence)
                .is_empty()
            && self
                .verification_failures
                .blocking(&task_root, self.mutation_sequence)
                .is_none()
    }

    /// Notes for every current-revision failure shown to pre-exist the task
    /// (see [`preexisting_note`]). Empty when there are none.
    pub(crate) fn preexisting_failure_notes(&self) -> Vec<String> {
        self.verification_failures
            .preexisting(&self.verification_task_root(), self.mutation_sequence)
            .into_iter()
            .filter_map(|r| {
                r.attribution
                    .as_ref()
                    .map(|a| preexisting_note(&r.check_id, a))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/preexisting_failure/preexisting_failure_test.rs"]
mod tests;
