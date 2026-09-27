//! Deterministic repository inventory for reviews (no model call).
//!
//! A review that reads two files and answers is not a review (live, 0.9.3:
//! "can you review the selfware core" ended after `directory_tree` + one
//! more call, accepted as done). Before a review can claim anything, the run
//! has to know what the repository contains and what "all relevant files"
//! means. This module answers that from the files alone:
//!
//! - totals and a per-language breakdown (files, lines, bytes);
//! - the largest code files;
//! - the most central files, by **import in-degree**: the number of distinct
//!   files whose import statements resolve to the file (Rust `use` paths via
//!   the evolve graph's parser, Python `import`/`from`, relative JS/TS
//!   specifiers, Go package imports inside the module);
//! - entry points (`src/main.rs`, `src/lib.rs`, `src/bin/*`, `__main__.py`,
//!   `package.json` `main`/`bin`, `cmd/*/main.go`, …);
//! - a review scope mapped from the task text, and a reading plan over it:
//!   entry points → most central hubs → every remaining relevant file.
//!
//! Discovery reuses the evolve graph's walker policy
//! ([`crate::evolve::graph`]: build output, dependency, cache and VCS
//! directories pruned, credential files skipped, the same test/example
//! partition) and is gitignore-aware through `git ls-files` when the root is
//! inside a git work tree. Binary files are counted by size only.

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};

use crate::evolve::graph::{
    classify_repository_paths, is_tooling_module_path, repository_file_walk, rust_use_paths,
    RepositoryFileClass,
};

/// What "most central" means, verbatim in every rendering.
pub const CENTRALITY_METRIC: &str =
    "import in-degree = distinct production files whose imports resolve to this file";

/// Entries shown in the "largest" and "most central" lists.
pub const TOP_N: usize = 10;

/// Files above this size are counted by bytes only (never opened in full,
/// never code for the review): generated dumps and data blobs.
const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;

/// Bytes sniffed for a NUL to call a file binary.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

/// How a file takes part in a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    Production,
    Test,
    Example,
    /// Machine-generated (`@generated`, `DO NOT EDIT`, `*.pb.go`, `*.min.js`, …).
    Generated,
}

/// Whether `.gitignore` rules were applied to the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitignoreStatus {
    /// `git ls-files --cached --others --exclude-standard` filtered the walk.
    Applied,
    /// The root is not inside a git work tree: only the built-in pruning ran.
    NotAGitRepository,
}

/// One text or binary file of the inventory.
#[derive(Debug, Clone, Serialize)]
pub struct InventoryFile {
    /// Repository-relative, `/`-separated.
    pub path: String,
    pub language: &'static str,
    /// Source code in a programming language (not docs, config or data).
    pub code: bool,
    pub role: FileRole,
    /// `str::lines().count()` of the (lossy UTF-8) content — the same count
    /// `file_read` reports as `total_lines`. 0 for binary / oversized files.
    pub lines: usize,
    pub bytes: u64,
    pub binary: bool,
    /// Distinct files importing this one (see [`CENTRALITY_METRIC`]).
    pub in_degree: usize,
    pub entry_point: bool,
}

/// Files / lines / bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub files: usize,
    pub lines: usize,
    pub bytes: u64,
}

impl Totals {
    fn add(&mut self, file: &InventoryFile) {
        self.files += 1;
        self.lines += file.lines;
        self.bytes += file.bytes;
    }
}

/// Per-language totals.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageStats {
    pub language: String,
    pub code: bool,
    pub files: usize,
    pub lines: usize,
    pub bytes: u64,
}

/// A file named in a top-N list.
#[derive(Debug, Clone, Serialize)]
pub struct FileRef {
    pub path: String,
    pub lines: usize,
    pub bytes: u64,
    pub in_degree: usize,
    pub role: FileRole,
}

/// The deterministic inventory of one repository.
#[derive(Debug, Clone, Serialize)]
pub struct RepoInventory {
    pub root: String,
    pub gitignore: GitignoreStatus,
    pub totals: Totals,
    /// Of `totals`: binary files (counted by bytes, never read).
    pub binary_files: usize,
    /// Code files of every role (production, tests, examples, generated).
    pub code: Totals,
    pub languages: Vec<LanguageStats>,
    pub largest_code_files: Vec<FileRef>,
    pub centrality_metric: &'static str,
    /// Distinct (importer, imported) file pairs the graph resolved.
    pub import_edges: usize,
    pub most_central: Vec<FileRef>,
    pub entry_points: Vec<String>,
    #[serde(skip)]
    pub files: Vec<InventoryFile>,
}

/// What a review covers, mapped from the task text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewScope {
    /// Human label ("whole repository", "src/agent", "core: …").
    pub label: String,
    /// Path prefixes (repository-relative) the scope is limited to; empty
    /// means the whole repository.
    pub prefixes: Vec<String>,
    /// The "core" mapping: production code under `src/` minus selfware's
    /// tooling modules (the evolve graph's Code layer).
    pub core: bool,
    pub include_tests: bool,
    pub include_examples: bool,
}

impl ReviewScope {
    /// The whole repository, production code only.
    pub fn whole_repository() -> Self {
        Self {
            label: "whole repository".to_string(),
            prefixes: Vec::new(),
            core: false,
            include_tests: false,
            include_examples: false,
        }
    }

    fn contains(&self, file: &InventoryFile) -> bool {
        let role_ok = match file.role {
            FileRole::Production => true,
            FileRole::Test => self.include_tests,
            FileRole::Example => self.include_examples,
            FileRole::Generated => false,
        };
        if !file.code || file.binary || !role_ok {
            return false;
        }
        if self.core && !is_core_path(&file.path) {
            return false;
        }
        self.prefixes.is_empty()
            || self
                .prefixes
                .iter()
                .any(|p| file.path == *p || file.path.starts_with(&format!("{p}/")))
    }
}

/// Why a file sits where it does in the reading plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanReason {
    EntryPoint,
    Hub,
    Remaining,
}

/// One file of the reading plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanEntry {
    pub path: String,
    pub lines: usize,
    pub in_degree: usize,
    pub reason: PlanReason,
}

/// Scope + ordered plan over the relevant files.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewPlan {
    pub scope: ReviewScope,
    pub relevant: Totals,
    pub plan: Vec<PlanEntry>,
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

impl RepoInventory {
    /// Walk `root` and build the inventory. Deterministic: same tree, same
    /// output (paths sorted, ties broken by path).
    pub fn scan(root: &Path) -> Result<Self> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut paths = repository_file_walk(&root)?;
        let gitignore = match git_visible_files(&root) {
            Some(visible) => {
                paths.retain(|p| {
                    p.strip_prefix(&root)
                        .map(|rel| visible.contains(&slash_path(rel)))
                        .unwrap_or(false)
                });
                GitignoreStatus::Applied
            }
            None => GitignoreStatus::NotAGitRepository,
        };

        // Contents of text files, kept only for the import pass.
        let mut contents: Vec<Option<String>> = Vec::with_capacity(paths.len());
        let mut files: Vec<InventoryFile> = Vec::with_capacity(paths.len());
        for path in &paths {
            let rel = slash_path(path.strip_prefix(&root).unwrap_or(path));
            let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let language = language_of(&rel);
            let (text, binary) = if bytes > MAX_TEXT_BYTES {
                (None, false)
            } else {
                match std::fs::read(path) {
                    Ok(raw) => {
                        let sniff = &raw[..raw.len().min(BINARY_SNIFF_BYTES)];
                        if sniff.contains(&0) {
                            (None, true)
                        } else {
                            (Some(String::from_utf8_lossy(&raw).into_owned()), false)
                        }
                    }
                    Err(_) => (None, false),
                }
            };
            let lines = text.as_deref().map(|t| t.lines().count()).unwrap_or(0);
            let generated = text.as_deref().is_some_and(|t| is_generated(&rel, t));
            files.push(InventoryFile {
                code: is_code_language(language) && text.is_some(),
                path: rel,
                language: if binary { "Binary" } else { language },
                role: if generated {
                    FileRole::Generated
                } else {
                    FileRole::Production
                },
                lines,
                bytes,
                binary,
                in_degree: 0,
                entry_point: false,
            });
            contents.push(text);
        }

        // Test / example partition — the evolve graph's.
        let classes = classify_repository_paths(&root, &paths)?;
        for (file, class) in files.iter_mut().zip(classes) {
            if file.role == FileRole::Generated {
                continue;
            }
            file.role = match class {
                RepositoryFileClass::Production => FileRole::Production,
                RepositoryFileClass::Test => FileRole::Test,
                RepositoryFileClass::Example => FileRole::Example,
            };
        }

        let importers = import_graph(&root, &files, &contents);
        let mut import_edges = 0;
        for (target, from) in importers.iter().enumerate() {
            files[target].in_degree = from.len();
            import_edges += from.len();
        }

        let entry_points = detect_entry_points(&files, &contents);
        for file in &mut files {
            file.entry_point = entry_points.contains(&file.path);
        }

        let mut totals = Totals::default();
        let mut code = Totals::default();
        let mut binary_files = 0;
        let mut by_language: BTreeMap<&'static str, LanguageStats> = BTreeMap::new();
        for file in &files {
            totals.add(file);
            if file.binary {
                binary_files += 1;
            }
            if file.code {
                code.add(file);
            }
            let entry = by_language
                .entry(file.language)
                .or_insert_with(|| LanguageStats {
                    language: file.language.to_string(),
                    code: file.code,
                    files: 0,
                    lines: 0,
                    bytes: 0,
                });
            entry.files += 1;
            entry.lines += file.lines;
            entry.bytes += file.bytes;
            entry.code |= file.code;
        }
        let mut languages: Vec<LanguageStats> = by_language.into_values().collect();
        languages.sort_by(|a, b| {
            b.lines
                .cmp(&a.lines)
                .then(b.bytes.cmp(&a.bytes))
                .then(a.language.cmp(&b.language))
        });

        let file_ref = |f: &InventoryFile| FileRef {
            path: f.path.clone(),
            lines: f.lines,
            bytes: f.bytes,
            in_degree: f.in_degree,
            role: f.role,
        };
        let mut largest: Vec<&InventoryFile> = files
            .iter()
            .filter(|f| f.code && f.role != FileRole::Generated)
            .collect();
        largest.sort_by(|a, b| {
            b.lines
                .cmp(&a.lines)
                .then(b.bytes.cmp(&a.bytes))
                .then(a.path.cmp(&b.path))
        });
        let largest_code_files = largest.into_iter().take(TOP_N).map(file_ref).collect();

        let mut central: Vec<&InventoryFile> = files
            .iter()
            .filter(|f| f.code && f.role == FileRole::Production && f.in_degree > 0)
            .collect();
        central.sort_by(|a, b| {
            b.in_degree
                .cmp(&a.in_degree)
                .then(b.lines.cmp(&a.lines))
                .then(a.path.cmp(&b.path))
        });
        let most_central = central.into_iter().take(TOP_N).map(file_ref).collect();

        let mut entry_points: Vec<String> = entry_points.into_iter().collect();
        // Library/binary roots first, then shallow before deep.
        entry_points.sort_by_key(|p| (entry_rank(p), p.matches('/').count(), p.clone()));

        Ok(Self {
            root: root.display().to_string(),
            gitignore,
            totals,
            binary_files,
            code,
            languages,
            largest_code_files,
            centrality_metric: CENTRALITY_METRIC,
            import_edges,
            most_central,
            entry_points,
            files,
        })
    }

    /// The reading plan over `scope`: entry points, then the most central
    /// hubs (up to [`TOP_N`], by in-degree), then every remaining relevant
    /// file by path (keeps a module's files together).
    pub fn review_plan(&self, scope: ReviewScope) -> ReviewPlan {
        let relevant: Vec<&InventoryFile> =
            self.files.iter().filter(|f| scope.contains(f)).collect();
        let mut totals = Totals::default();
        for f in &relevant {
            totals.add(f);
        }
        let entry = |f: &InventoryFile, reason| PlanEntry {
            path: f.path.clone(),
            lines: f.lines,
            in_degree: f.in_degree,
            reason,
        };
        let mut plan: Vec<PlanEntry> = Vec::with_capacity(relevant.len());
        let mut placed: HashSet<&str> = HashSet::new();
        // Entry points: library roots first (lib.rs / __init__), then the rest.
        let mut entries: Vec<&InventoryFile> =
            relevant.iter().copied().filter(|f| f.entry_point).collect();
        entries.sort_by_key(|f| (entry_rank(&f.path), f.path.clone()));
        for f in entries {
            placed.insert(f.path.as_str());
            plan.push(entry(f, PlanReason::EntryPoint));
        }
        let mut hubs: Vec<&InventoryFile> = relevant
            .iter()
            .copied()
            .filter(|f| f.in_degree > 0 && !placed.contains(f.path.as_str()))
            .collect();
        hubs.sort_by(|a, b| {
            b.in_degree
                .cmp(&a.in_degree)
                .then(b.lines.cmp(&a.lines))
                .then(a.path.cmp(&b.path))
        });
        for f in hubs.into_iter().take(TOP_N) {
            placed.insert(f.path.as_str());
            plan.push(entry(f, PlanReason::Hub));
        }
        for f in &relevant {
            if !placed.contains(f.path.as_str()) {
                plan.push(entry(f, PlanReason::Remaining));
            }
        }
        ReviewPlan {
            scope,
            relevant: totals,
            plan,
        }
    }

    /// The inventory as the user sees it at the start of a review (and from
    /// `selfware review`): header, totals, languages, largest, most central,
    /// entry points, and — with a plan — the scope and reading plan.
    pub fn render_text(&self, plan: Option<&ReviewPlan>) -> String {
        let mut out = Vec::new();
        out.push(format!("Repository inventory — {}", self.root));
        let ignore = match self.gitignore {
            GitignoreStatus::Applied => "gitignore applied",
            GitignoreStatus::NotAGitRepository => "not a git repository: gitignore NOT applied",
        };
        out.push(format!(
            "  files: {} ({} code, {} binary) · lines: {} · size: {}   [{ignore}; build/dependency/cache dirs skipped]",
            group(self.totals.files),
            group(self.code.files),
            group(self.binary_files),
            group(self.totals.lines),
            human_bytes(self.totals.bytes),
        ));
        out.push("  languages (by lines):".to_string());
        for lang in self.languages.iter().take(8) {
            out.push(format!(
                "    {:<12} {:>6} files {:>9} lines {:>10}{}",
                lang.language,
                group(lang.files),
                group(lang.lines),
                human_bytes(lang.bytes),
                if lang.code { "" } else { "  (not code)" }
            ));
        }
        if self.languages.len() > 8 {
            let rest = &self.languages[8..];
            out.push(format!(
                "    +{} more: {} files, {} lines",
                rest.len(),
                group(rest.iter().map(|l| l.files).sum()),
                group(rest.iter().map(|l| l.lines).sum())
            ));
        }
        if !self.largest_code_files.is_empty() {
            out.push("  largest code files:".to_string());
            for (i, f) in self.largest_code_files.iter().enumerate() {
                out.push(format!(
                    "    {:>2}. {} — {} lines, {}{}",
                    i + 1,
                    f.path,
                    group(f.lines),
                    human_bytes(f.bytes),
                    role_suffix(f.role)
                ));
            }
        }
        out.push(format!(
            "  most central ({}; {} import edges resolved):",
            self.centrality_metric,
            group(self.import_edges)
        ));
        if self.most_central.is_empty() {
            out.push("    (no resolvable imports between files)".to_string());
        }
        for (i, f) in self.most_central.iter().enumerate() {
            out.push(format!(
                "    {:>2}. {} — imported by {} files ({} lines)",
                i + 1,
                f.path,
                group(f.in_degree),
                group(f.lines)
            ));
        }
        out.push(format!(
            "  entry points: {}",
            if self.entry_points.is_empty() {
                "none detected".to_string()
            } else {
                list_with_more(&self.entry_points, 8)
            }
        ));
        if let Some(plan) = plan {
            out.push(format!(
                "  review scope: {} → {} relevant files, {} lines{}",
                plan.scope.label,
                group(plan.relevant.files),
                group(plan.relevant.lines),
                excluded_note(&plan.scope)
            ));
            let shown: Vec<String> = plan
                .plan
                .iter()
                .take(12)
                .enumerate()
                .map(|(i, e)| format!("{}. {}{}", i + 1, e.path, reason_tag(e)))
                .collect();
            let more = plan.plan.len().saturating_sub(shown.len());
            out.push(format!(
                "  reading plan: {}{}",
                shown.join(" · "),
                if more > 0 {
                    format!(" · … (+{more} more)")
                } else {
                    String::new()
                }
            ));
        }
        out.join("\n")
    }

    /// The compact form injected into the model's context at review start,
    /// kept within `max_tokens` (measured with
    /// [`crate::token_count::estimate_content_tokens`]): the header lines,
    /// then as much of the reading plan as fits, the rest counted.
    pub fn render_compact(&self, plan: &ReviewPlan, max_tokens: usize) -> String {
        use crate::token_count::estimate_content_tokens;
        let mut head = vec![format!(
            "Repository: {} files ({} code), {} lines, {}; review scope: {} → {} relevant files, {} lines{}.",
            group(self.totals.files),
            group(self.code.files),
            group(self.totals.lines),
            human_bytes(self.totals.bytes),
            plan.scope.label,
            group(plan.relevant.files),
            group(plan.relevant.lines),
            excluded_note(&plan.scope),
        )];
        let langs: Vec<String> = self
            .languages
            .iter()
            .filter(|l| l.code)
            .take(5)
            .map(|l| format!("{} {} files/{} lines", l.language, l.files, l.lines))
            .collect();
        if !langs.is_empty() {
            head.push(format!("Languages: {}.", langs.join(", ")));
        }
        if !self.most_central.is_empty() {
            head.push(format!(
                "Most central ({}): {}.",
                self.centrality_metric,
                self.most_central
                    .iter()
                    .take(5)
                    .map(|f| format!("{} ({})", f.path, f.in_degree))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        head.push(
            "Reading plan (path, lines) — read every file, in this order, with file_read:"
                .to_string(),
        );
        let mut text = head.join("\n");
        let mut shown = 0;
        for e in &plan.plan {
            let line = format!("\n- {} ({}){}", e.path, e.lines, reason_tag(e));
            let rest = plan.plan.len() - shown - 1;
            let tail = if rest > 0 {
                format!("\n… +{rest} more files (the per-turn review status names the next ones)")
            } else {
                String::new()
            };
            if estimate_content_tokens(&format!("{text}{line}{tail}")) > max_tokens {
                break;
            }
            text.push_str(&line);
            shown += 1;
        }
        let rest = plan.plan.len() - shown;
        if rest > 0 {
            text.push_str(&format!(
                "\n… +{rest} more files (the per-turn review status names the next ones)"
            ));
        }
        text
    }
}

fn excluded_note(scope: &ReviewScope) -> String {
    let mut excluded = Vec::new();
    if !scope.include_tests {
        excluded.push("tests");
    }
    if !scope.include_examples {
        excluded.push("examples");
    }
    excluded.push("generated");
    format!(" ({} excluded)", excluded.join("/"))
}

fn reason_tag(e: &PlanEntry) -> String {
    match e.reason {
        PlanReason::EntryPoint => " [entry]".to_string(),
        PlanReason::Hub => format!(" [hub, imported by {}]", e.in_degree),
        PlanReason::Remaining => String::new(),
    }
}

fn role_suffix(role: FileRole) -> &'static str {
    match role {
        FileRole::Production => "",
        FileRole::Test => " (test)",
        FileRole::Example => " (example)",
        FileRole::Generated => " (generated)",
    }
}

fn list_with_more(items: &[String], max: usize) -> String {
    let shown = items
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > max {
        format!("{shown} (+{} more)", items.len() - max)
    } else {
        shown
    }
}

/// Library roots before binaries in the plan (`lib.rs`, `__init__.py`,
/// `index.*` first).
fn entry_rank(path: &str) -> u8 {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "lib.rs" | "__init__.py" => 0,
        "main.rs" | "__main__.py" | "main.go" | "main.py" => 1,
        _ => 2,
    }
}

/// `1234567` → `1,234,567`.
pub fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Bytes as B / KB / MB (1024-based, one decimal).
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Files git would show for `root` (tracked + untracked-not-ignored), as
/// root-relative `/` paths; `None` outside a git work tree or when git
/// fails. Spawned with the sanitized environment (AGENTS.md rule 5).
fn git_visible_files(root: &Path) -> Option<HashSet<String>> {
    use crate::safety::process_env::SanitizedEnvExt;
    let output = std::process::Command::new("git")
        .sanitized_env()
        .args([
            "-c",
            "core.quotepath=off",
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|&b| b == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Languages, generated files
// ---------------------------------------------------------------------------

fn language_of(rel: &str) -> &'static str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name {
        "Makefile" | "makefile" | "GNUmakefile" => return "Makefile",
        "Dockerfile" => return "Dockerfile",
        _ => {}
    }
    let ext = name
        .rsplit_once('.')
        .map(|(stem, ext)| if stem.is_empty() { "" } else { ext })
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "Rust",
        "py" | "pyw" | "pyi" => "Python",
        "js" | "mjs" | "cjs" | "jsx" => "JavaScript",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "scala" => "Scala",
        "c" | "h" => "C",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "dart" => "Dart",
        "ex" | "exs" => "Elixir",
        "hs" => "Haskell",
        "ml" | "mli" => "OCaml",
        "zig" => "Zig",
        "lua" => "Lua",
        "pl" | "pm" => "Perl",
        "lean" => "Lean",
        "sh" | "bash" | "zsh" => "Shell",
        "ps1" => "PowerShell",
        "sql" => "SQL",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "swl" => "SWL",
        "proto" => "Protobuf",
        "md" | "markdown" | "rst" | "adoc" => "Markdown",
        "txt" => "Text",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "json" | "jsonc" | "jsonl" | "ndjson" => "JSON",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "xml" => "XML",
        "svg" => "SVG",
        "csv" | "tsv" => "CSV",
        "lock" => "Lockfile",
        _ => "Other",
    }
}

fn is_code_language(language: &str) -> bool {
    !matches!(
        language,
        "Markdown"
            | "Text"
            | "TOML"
            | "YAML"
            | "JSON"
            | "HTML"
            | "CSS"
            | "XML"
            | "SVG"
            | "CSV"
            | "Lockfile"
            | "Other"
            | "Binary"
            | "Makefile"
            | "Dockerfile"
            | "Protobuf"
    )
}

fn is_generated(rel: &str, text: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_ascii_lowercase();
    if name.ends_with(".pb.go")
        || name.ends_with("_pb2.py")
        || name.ends_with("_pb2_grpc.py")
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.contains(".generated.")
        || name.ends_with(".g.dart")
        || name.ends_with(".designer.cs")
    {
        return true;
    }
    let head: String = text.lines().take(5).collect::<Vec<_>>().join("\n");
    let head = head.to_ascii_lowercase();
    head.contains("@generated")
        || head.contains("do not edit")
        || head.contains("code generated")
        || head.contains("auto-generated")
        || head.contains("autogenerated")
}

/// selfware's "core": production code under `src/` outside the tooling
/// modules (the evolve graph's Code layer).
fn is_core_path(rel: &str) -> bool {
    rel.starts_with("src/") && !is_tooling_module_path(Path::new(rel))
}

// ---------------------------------------------------------------------------
// Scope mapping
// ---------------------------------------------------------------------------

/// Words that never name a directory in a review request.
const SCOPE_STOPWORDS: &[&str] = &[
    "the",
    "this",
    "that",
    "these",
    "those",
    "and",
    "for",
    "with",
    "all",
    "any",
    "can",
    "you",
    "please",
    "review",
    "audit",
    "assess",
    "code",
    "codebase",
    "repo",
    "repository",
    "project",
    "bugs",
    "bug",
    "file",
    "files",
    "line",
    "lines",
    "cite",
    "not",
    "don",
    "dont",
    "our",
    "its",
    "into",
    "from",
    "about",
    "quality",
    "whole",
    "entire",
    "full",
    "issues",
    "problems",
    "look",
    "check",
    "find",
    "report",
    "findings",
    "security",
    "performance",
    "only",
    "just",
    "then",
    "src",
    "test",
    "tests",
    "testing",
    "example",
    "examples",
];

/// Map the task text to a review scope over this inventory:
///
/// 1. explicit paths / directory names that exist (`src/agent`, `agent`,
///    `parser.rs`) → those prefixes;
/// 2. "core" with no directory of that name → selfware's core (production
///    code under `src/` minus the tooling modules);
/// 3. otherwise the whole repository.
///
/// Tests are in scope only when the task mentions them; examples likewise.
pub fn resolve_review_scope(task: &str, inventory: &RepoInventory) -> ReviewScope {
    let lower = task.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | ';' | ':' | '(' | ')' | '"' | '\'' | '`' | '?' | '!'
                )
        })
        .map(|w| w.trim_end_matches(['.', ',']))
        .filter(|w| !w.is_empty())
        .collect();
    let include_tests = words
        .iter()
        .any(|w| matches!(*w, "test" | "tests" | "testing" | "test-suite" | "specs"));
    let include_examples = words.iter().any(|w| matches!(*w, "example" | "examples"));

    // Directory set (every ancestor of every file) and file paths.
    let mut dirs: HashSet<String> = HashSet::new();
    for f in &inventory.files {
        let mut acc = String::new();
        let parts: Vec<&str> = f.path.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(part);
            dirs.insert(acc.clone());
        }
    }
    let mut prefixes: Vec<String> = Vec::new();
    let push = |p: String, prefixes: &mut Vec<String>| {
        if !prefixes.contains(&p) {
            prefixes.push(p);
        }
    };
    for raw in &words {
        let w = raw.trim_start_matches("./").trim_end_matches('/');
        if w.len() < 3 || SCOPE_STOPWORDS.contains(&w) {
            continue;
        }
        if w.contains('/') {
            if dirs.contains(w) || inventory.files.iter().any(|f| f.path == w) {
                push(w.to_string(), &mut prefixes);
            }
            continue;
        }
        if w.contains('.') {
            for f in &inventory.files {
                if f.path == w || f.path.ends_with(&format!("/{w}")) {
                    push(f.path.clone(), &mut prefixes);
                }
            }
            continue;
        }
        // A bare word: directories with that basename (shallowest first).
        let mut matched: Vec<&String> = dirs
            .iter()
            .filter(|d| d.rsplit('/').next() == Some(w))
            .collect();
        matched.sort_by_key(|d| (d.matches('/').count(), (*d).clone()));
        if let Some(depth) = matched.first().map(|d| d.matches('/').count()) {
            for d in matched
                .into_iter()
                .filter(|d| d.matches('/').count() == depth)
            {
                push(d.clone(), &mut prefixes);
            }
        }
    }
    if !prefixes.is_empty() {
        prefixes.sort();
        return ReviewScope {
            label: prefixes.join(", "),
            prefixes,
            core: false,
            include_tests,
            include_examples,
        };
    }
    let asks_core = words.iter().any(|w| matches!(*w, "core" | "kernel"));
    if asks_core
        && inventory
            .files
            .iter()
            .any(|f| f.code && is_core_path(&f.path))
    {
        return ReviewScope {
            label: "core: production code under src/ minus tooling modules (ui, output, testing, bin, …)"
                .to_string(),
            prefixes: Vec::new(),
            core: true,
            include_tests,
            include_examples,
        };
    }
    ReviewScope {
        include_tests,
        include_examples,
        ..ReviewScope::whole_repository()
    }
}

// ---------------------------------------------------------------------------
// Import graph
// ---------------------------------------------------------------------------

/// `importers[i]` = distinct indices of files whose imports resolve to file
/// `i` (self-imports excluded).
fn import_graph(
    root: &Path,
    files: &[InventoryFile],
    contents: &[Option<String>],
) -> Vec<HashSet<usize>> {
    let mut importers: Vec<HashSet<usize>> = vec![HashSet::new(); files.len()];
    let by_path: HashMap<&str, usize> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();
    // Tests and examples import everything through the public API; only
    // production importers say how central a file is to the code itself.
    let mut add = |from: usize, to: usize| {
        if from != to && files[from].role == FileRole::Production {
            importers[to].insert(from);
        }
    };
    for (from, to) in rust_edges(root, files, contents, &by_path) {
        add(from, to);
    }
    for (from, to) in python_edges(files, contents) {
        add(from, to);
    }
    for (from, to) in js_edges(files, contents, &by_path) {
        add(from, to);
    }
    for (from, to) in go_edges(root, files, contents) {
        add(from, to);
    }
    importers
}

/// Lexically normalize `a/b/../c` → `a/c`; `None` when it escapes the root.
fn normalize_rel(path: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            p => out.push(p),
        }
    }
    Some(out.join("/"))
}

fn parent_dir(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn join_rel(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    }
}

// --- Rust -------------------------------------------------------------------

struct RustCrate {
    dir: String,
    name: Option<String>,
    /// Module path (`a::b`, "" = crate root) → file index.
    modules: HashMap<String, usize>,
}

/// The crate owning `rel` (`crates` sorted deepest first).
fn crate_index(crates: &[RustCrate], rel: &str) -> Option<usize> {
    crates
        .iter()
        .position(|c| c.dir.is_empty() || rel == c.dir || rel.starts_with(&format!("{}/", c.dir)))
}

fn rust_edges(
    root: &Path,
    files: &[InventoryFile],
    contents: &[Option<String>],
    by_path: &HashMap<&str, usize>,
) -> Vec<(usize, usize)> {
    // Crates: every directory holding a Cargo.toml with a [package].
    let mut crates: Vec<RustCrate> = Vec::new();
    for f in files {
        if f.path == "Cargo.toml" || f.path.ends_with("/Cargo.toml") {
            let dir = parent_dir(&f.path).to_string();
            let name = std::fs::read_to_string(root.join(&f.path))
                .ok()
                .and_then(|t| t.parse::<toml::Table>().ok())
                .and_then(|t| {
                    t.get("package")?
                        .get("name")?
                        .as_str()
                        .map(|n| n.replace('-', "_"))
                });
            if name.is_some() || by_path.contains_key(join_rel(&dir, "src/lib.rs").as_str()) {
                crates.push(RustCrate {
                    dir,
                    name,
                    modules: HashMap::new(),
                });
            }
        }
    }
    if crates.is_empty() {
        return Vec::new();
    }
    // Deepest crate dir first, so a nested crate wins over the workspace root.
    crates.sort_by_key(|c| {
        std::cmp::Reverse(c.dir.matches('/').count() + usize::from(!c.dir.is_empty()))
    });

    // Module ids of every library-side file (`<crate>/src/**`, not src/bin).
    let mut file_module: HashMap<usize, (usize, Vec<String>)> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if f.language != "Rust" {
            continue;
        }
        let Some(ci) = crate_index(&crates, &f.path) else {
            continue;
        };
        let within = if crates[ci].dir.is_empty() {
            f.path.as_str()
        } else {
            &f.path[crates[ci].dir.len() + 1..]
        };
        let Some(src_rel) = within.strip_prefix("src/") else {
            continue;
        };
        if src_rel.starts_with("bin/") {
            continue;
        }
        let mut parts: Vec<String> = src_rel
            .trim_end_matches(".rs")
            .split('/')
            .map(str::to_string)
            .collect();
        if parts.last().is_some_and(|p| p == "mod") {
            parts.pop();
        }
        if parts.as_slice() == ["lib"] || parts.as_slice() == ["main"] {
            parts.clear();
        }
        let key = parts.join("::");
        // lib.rs owns the crate root over main.rs.
        let is_main = src_rel == "main.rs";
        let slot = crates[ci].modules.entry(key).or_insert(i);
        if !is_main && files[*slot].path.ends_with("src/main.rs") {
            *slot = i;
        }
        file_module.insert(i, (ci, parts));
    }
    let names: HashMap<String, usize> = crates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.name.clone().map(|n| (n, i)))
        .collect();

    let resolve = |ci: usize, parts: &[String]| -> Option<usize> {
        let mut candidate = parts.to_vec();
        loop {
            if let Some(&idx) = crates[ci].modules.get(&candidate.join("::")) {
                return Some(idx);
            }
            candidate.pop()?;
        }
    };

    let mut edges = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if f.language != "Rust" {
            continue;
        }
        let Some(content) = contents[i].as_deref() else {
            continue;
        };
        let own = file_module.get(&i).cloned();
        let own_crate = own
            .as_ref()
            .map(|(c, _)| *c)
            .or_else(|| crate_index(&crates, &f.path));
        for path in rust_use_paths(content) {
            let Some(first) = path.first().map(String::as_str) else {
                continue;
            };
            let target = match first {
                "crate" => own_crate.and_then(|c| resolve(c, &path[1..])),
                "self" | "super" => own.as_ref().and_then(|(c, module)| {
                    let mut base = module.clone();
                    let mut rest = &path[..];
                    if rest.first().is_some_and(|p| p == "self") {
                        rest = &rest[1..];
                    }
                    while rest.first().is_some_and(|p| p == "super") {
                        base.pop()?;
                        rest = &rest[1..];
                    }
                    base.extend(rest.iter().cloned());
                    resolve(*c, &base)
                }),
                name => match names.get(name) {
                    Some(&c) => resolve(c, &path[1..]),
                    // 2018 uniform paths: a child module in scope.
                    None => own.as_ref().and_then(|(c, module)| {
                        let mut child = module.clone();
                        child.push(name.to_string());
                        crates[*c]
                            .modules
                            .contains_key(&child.join("::"))
                            .then(|| {
                                let mut full = module.clone();
                                full.extend(path.iter().cloned());
                                resolve(*c, &full)
                            })
                            .flatten()
                    }),
                },
            };
            if let Some(t) = target {
                edges.push((i, t));
            }
        }
    }
    edges
}

// --- Python -----------------------------------------------------------------

static PY_IMPORT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*import\s+([\w\.]+(?:\s+as\s+\w+)?(?:\s*,\s*[\w\.]+(?:\s+as\s+\w+)?)*)")
        .expect("valid regex")
});
static PY_FROM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*from\s+(\.*)([\w\.]*)\s+import\s+\(?\s*([\w\s,\*]+)").expect("valid regex")
});

/// Dotted module name of a Python file under `base` ("" = repo root):
/// `pkg/mod.py` → `pkg.mod`, `pkg/__init__.py` → `pkg`.
fn python_module_name(rel: &str) -> Option<String> {
    let stem = rel
        .strip_suffix(".py")
        .or_else(|| rel.strip_suffix(".pyi"))?;
    let mut parts: Vec<&str> = stem.split('/').collect();
    if parts.last() == Some(&"__init__") {
        parts.pop();
    }
    (!parts.is_empty()).then(|| parts.join("."))
}

fn python_edges(files: &[InventoryFile], contents: &[Option<String>]) -> Vec<(usize, usize)> {
    // Module name → file, rooted at the repository and at src/, lib/.
    let mut modules: HashMap<String, usize> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if f.language != "Python" {
            continue;
        }
        for base in ["", "src/", "lib/", "python/"] {
            if let Some(rel) = f.path.strip_prefix(base) {
                if let Some(name) = python_module_name(rel) {
                    modules.entry(name).or_insert(i);
                }
            }
        }
    }
    if modules.is_empty() {
        return Vec::new();
    }
    let resolve_longest = |dotted: &str| -> Option<usize> {
        let mut parts: Vec<&str> = dotted.split('.').filter(|p| !p.is_empty()).collect();
        while !parts.is_empty() {
            if let Some(&i) = modules.get(&parts.join(".")) {
                return Some(i);
            }
            parts.pop();
        }
        None
    };
    let mut edges = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if f.language != "Python" {
            continue;
        }
        let Some(content) = contents[i].as_deref() else {
            continue;
        };
        let own = python_module_name(&f.path).unwrap_or_default();
        let is_package = f.path.ends_with("__init__.py");
        for line in content.lines() {
            if let Some(c) = PY_IMPORT.captures(line) {
                for item in c[1].split(',') {
                    let module = item.split_whitespace().next().unwrap_or_default();
                    if let Some(t) = resolve_longest(module) {
                        edges.push((i, t));
                    }
                }
            } else if let Some(c) = PY_FROM.captures(line) {
                let level = c[1].len();
                let module = &c[2];
                let base = if level == 0 {
                    String::new()
                } else {
                    let mut pkg: Vec<&str> = own.split('.').filter(|p| !p.is_empty()).collect();
                    if !is_package {
                        pkg.pop();
                    }
                    for _ in 1..level {
                        pkg.pop();
                    }
                    pkg.join(".")
                };
                let target_module = [base.as_str(), module]
                    .iter()
                    .filter(|p| !p.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(".");
                let mut resolved = false;
                for name in c[3].split(',') {
                    let name = name.split_whitespace().next().unwrap_or_default();
                    if name.is_empty() || name == "*" {
                        continue;
                    }
                    let sub = if target_module.is_empty() {
                        name.to_string()
                    } else {
                        format!("{target_module}.{name}")
                    };
                    if let Some(&t) = modules.get(&sub) {
                        edges.push((i, t));
                        resolved = true;
                    }
                }
                if !resolved && !target_module.is_empty() {
                    if let Some(t) = resolve_longest(&target_module) {
                        edges.push((i, t));
                    }
                }
            }
        }
    }
    edges
}

// --- JavaScript / TypeScript ------------------------------------------------

static JS_SPECIFIER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\s*\(\s*|\bexport\s*\*\s*from\s*)['"](\.{1,2}/[^'"]*)['"]"#,
    )
    .expect("valid regex")
});

const JS_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "vue", "svelte",
];

fn js_edges(
    files: &[InventoryFile],
    contents: &[Option<String>],
    by_path: &HashMap<&str, usize>,
) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if !matches!(f.language, "JavaScript" | "TypeScript" | "Vue" | "Svelte") {
            continue;
        }
        let Some(content) = contents[i].as_deref() else {
            continue;
        };
        let dir = parent_dir(&f.path);
        for c in JS_SPECIFIER.captures_iter(content) {
            let Some(base) = normalize_rel(&join_rel(dir, &c[1])) else {
                continue;
            };
            let mut candidates = vec![base.clone()];
            let stem = base
                .strip_suffix(".js")
                .or_else(|| base.strip_suffix(".jsx"))
                .or_else(|| base.strip_suffix(".mjs"));
            for ext in JS_EXTENSIONS {
                candidates.push(format!("{base}.{ext}"));
                if let Some(stem) = stem {
                    candidates.push(format!("{stem}.{ext}"));
                }
                candidates.push(join_rel(&base, &format!("index.{ext}")));
            }
            if let Some(&t) = candidates.iter().find_map(|c| by_path.get(c.as_str())) {
                edges.push((i, t));
            }
        }
    }
    edges
}

// --- Go ---------------------------------------------------------------------

static GO_IMPORT_SINGLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^\s*import\s+(?:[\w\.]+\s+)?"([^"]+)""#).expect("valid regex"));
static GO_IMPORT_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?s)\bimport\s*\((.*?)\)"#).expect("valid regex"));
static GO_QUOTED: Lazy<Regex> = Lazy::new(|| Regex::new(r#""([^"]+)""#).expect("valid regex"));

fn go_edges(
    root: &Path,
    files: &[InventoryFile],
    contents: &[Option<String>],
) -> Vec<(usize, usize)> {
    // go.mod dir → module path.
    let mut modules: Vec<(String, String)> = files
        .iter()
        .filter(|f| f.path == "go.mod" || f.path.ends_with("/go.mod"))
        .filter_map(|f| {
            let text = std::fs::read_to_string(root.join(&f.path)).ok()?;
            let module = text
                .lines()
                .find_map(|l| l.trim().strip_prefix("module "))?
                .trim()
                .trim_matches('"')
                .to_string();
            Some((parent_dir(&f.path).to_string(), module))
        })
        .collect();
    if modules.is_empty() {
        return Vec::new();
    }
    modules.sort_by_key(|(dir, _)| std::cmp::Reverse(dir.len()));
    // Package dir → non-test Go files.
    let mut packages: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if f.language == "Go" && !f.path.ends_with("_test.go") {
            packages.entry(parent_dir(&f.path)).or_default().push(i);
        }
    }
    let mut edges = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if f.language != "Go" {
            continue;
        }
        let Some(content) = contents[i].as_deref() else {
            continue;
        };
        let mut imports: Vec<String> = GO_IMPORT_SINGLE
            .captures_iter(content)
            .map(|c| c[1].to_string())
            .collect();
        for block in GO_IMPORT_BLOCK.captures_iter(content) {
            imports.extend(GO_QUOTED.captures_iter(&block[1]).map(|c| c[1].to_string()));
        }
        for import in imports {
            for (dir, module) in &modules {
                let rest = if import == *module {
                    Some("")
                } else {
                    import.strip_prefix(&format!("{module}/"))
                };
                if let Some(rest) = rest {
                    let pkg_dir = if rest.is_empty() {
                        dir.clone()
                    } else {
                        join_rel(dir, rest)
                    };
                    for &t in packages.get(pkg_dir.as_str()).into_iter().flatten() {
                        edges.push((i, t));
                    }
                    break;
                }
            }
        }
    }
    edges
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

fn detect_entry_points(files: &[InventoryFile], contents: &[Option<String>]) -> HashSet<String> {
    let by_path: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut out: HashSet<String> = HashSet::new();
    let add = |p: String, out: &mut HashSet<String>| {
        if let Some(p) = normalize_rel(&p) {
            if by_path.contains(p.as_str()) {
                out.insert(p);
            }
        }
    };
    for (i, f) in files.iter().enumerate() {
        if f.role != FileRole::Production || f.binary {
            continue;
        }
        let path = f.path.as_str();
        let name = path.rsplit('/').next().unwrap_or(path);
        let dir = parent_dir(path);
        let content = contents[i].as_deref().unwrap_or_default();
        let is_entry = match f.language {
            "Rust" => {
                path.ends_with("src/main.rs")
                    || path.ends_with("src/lib.rs")
                    || dir.ends_with("src/bin")
                    || (dir.contains("src/bin/") && name == "main.rs")
                    || path == "build.rs"
            }
            "Python" => {
                name == "__main__.py"
                    || (dir.is_empty()
                        && matches!(
                            name,
                            "main.py" | "app.py" | "manage.py" | "cli.py" | "wsgi.py" | "asgi.py"
                        ))
                    || content.contains("if __name__ == \"__main__\"")
                    || content.contains("if __name__ == '__main__'")
            }
            "Go" => content.contains("package main") && content.contains("func main("),
            "C" | "C++" => name.starts_with("main."),
            "JavaScript" | "TypeScript" => {
                (dir.is_empty() || dir == "src") && name.starts_with("index.")
            }
            _ => false,
        };
        if is_entry {
            out.insert(path.to_string());
        }
        if name == "package.json" {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
                for key in ["main", "module", "browser"] {
                    if let Some(s) = v.get(key).and_then(|m| m.as_str()) {
                        add(join_rel(dir, s), &mut out);
                    }
                }
                match v.get("bin") {
                    Some(serde_json::Value::String(s)) => add(join_rel(dir, s), &mut out),
                    Some(serde_json::Value::Object(map)) => {
                        for s in map.values().filter_map(|v| v.as_str()) {
                            add(join_rel(dir, s), &mut out);
                        }
                    }
                    _ => {}
                }
            }
        }
        if name == "Cargo.toml" {
            if let Ok(t) = content.parse::<toml::Table>() {
                for bin in t
                    .get("bin")
                    .and_then(|b| b.as_array())
                    .into_iter()
                    .flatten()
                {
                    if let Some(p) = bin.get("path").and_then(|p| p.as_str()) {
                        add(join_rel(dir, p), &mut out);
                    }
                }
            }
        }
        if name == "pyproject.toml" {
            if let Ok(t) = content.parse::<toml::Table>() {
                let scripts = t
                    .get("project")
                    .and_then(|p| p.get("scripts"))
                    .and_then(|s| s.as_table());
                for target in scripts.into_iter().flat_map(|s| s.values()) {
                    let Some(module) = target.as_str().and_then(|s| s.split(':').next()) else {
                        continue;
                    };
                    let rel = module.replace('.', "/");
                    for base in ["", "src/"] {
                        add(join_rel(dir, &format!("{base}{rel}.py")), &mut out);
                        add(join_rel(dir, &format!("{base}{rel}/__init__.py")), &mut out);
                    }
                }
            }
        }
    }
    // Configured paths only count when they are production files.
    out.retain(|p| {
        files
            .iter()
            .any(|f| f.path == *p && f.role == FileRole::Production)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// A small Rust crate with a known import structure:
    /// `util` is imported by lib, a, b (in-degree 3); `a` by b (1).
    fn rust_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(
            r,
            "Cargo.toml",
            "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n",
        );
        write(
            r,
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub mod util;\nuse crate::util::helper;\n",
        );
        write(
            r,
            "src/main.rs",
            "use demo_crate::a::run;\nfn main() { run(); }\n",
        );
        write(
            r,
            "src/a.rs",
            "use crate::util::helper;\npub fn run() { helper(); }\n",
        );
        write(
            r,
            "src/b/mod.rs",
            "use super::util;\nuse super::a::run;\npub fn go() {\n    util::helper();\n    run();\n}\n",
        );
        write(r, "src/util.rs", "pub fn helper() {}\n");
        write(
            r,
            "tests/it.rs",
            "use demo_crate::util::helper;\n#[test]\nfn t() { helper(); }\n",
        );
        write(r, "README.md", "# demo\n\nline\n");
        write(r, "assets/logo.bin", "\u{0}\u{1}\u{2}binary");
        write(
            r,
            "src/gen.rs",
            "// @generated by a tool\npub const X: u8 = 1;\n",
        );
        dir
    }

    #[test]
    fn inventory_counts_languages_sizes_and_roles() {
        let dir = rust_fixture();
        let inv = RepoInventory::scan(dir.path()).unwrap();
        // Not a git repo: honest status.
        assert_eq!(inv.gitignore, GitignoreStatus::NotAGitRepository);
        assert_eq!(inv.totals.files, 10);
        assert_eq!(inv.binary_files, 1);
        let rust = inv.languages.iter().find(|l| l.language == "Rust").unwrap();
        assert_eq!(rust.files, 7);
        // 4 + 2 + 2 + 6 + 1 + 3 + 2 lines.
        assert_eq!(rust.lines, 20);
        let md = inv
            .languages
            .iter()
            .find(|l| l.language == "Markdown")
            .unwrap();
        assert!(!md.code);
        assert_eq!(md.lines, 3);
        let role = |p: &str| inv.files.iter().find(|f| f.path == p).unwrap().role;
        assert_eq!(role("tests/it.rs"), FileRole::Test);
        assert_eq!(role("src/gen.rs"), FileRole::Generated);
        assert_eq!(role("src/a.rs"), FileRole::Production);
        // Bytes are the real file sizes.
        let util = inv.files.iter().find(|f| f.path == "src/util.rs").unwrap();
        assert_eq!(util.bytes, "pub fn helper() {}\n".len() as u64);
        // Largest code file first: src/b/mod.rs (6 lines); generated excluded.
        assert_eq!(inv.largest_code_files[0].path, "src/b/mod.rs");
        assert!(inv
            .largest_code_files
            .iter()
            .all(|f| f.path != "src/gen.rs"));
    }

    #[test]
    fn centrality_is_import_in_degree_in_order() {
        let dir = rust_fixture();
        let inv = RepoInventory::scan(dir.path()).unwrap();
        let deg = |p: &str| inv.files.iter().find(|f| f.path == p).unwrap().in_degree;
        // util: lib (crate::util), a (crate::util), b (super::util); the
        // test importer (tests/it.rs) does not count.
        assert_eq!(deg("src/util.rs"), 3);
        // a: main (demo_crate::a), b (super::a).
        assert_eq!(deg("src/a.rs"), 2);
        assert_eq!(deg("src/b/mod.rs"), 0);
        // most_central: production only, highest in-degree first.
        assert_eq!(inv.most_central[0].path, "src/util.rs");
        assert_eq!(inv.most_central[1].path, "src/a.rs");
        assert_eq!(inv.centrality_metric, CENTRALITY_METRIC);
        assert_eq!(
            inv.entry_points,
            vec!["src/lib.rs".to_string(), "src/main.rs".to_string()]
        );
    }

    #[test]
    fn reading_plan_orders_entries_then_hubs_then_rest() {
        let dir = rust_fixture();
        let inv = RepoInventory::scan(dir.path()).unwrap();
        let plan = inv.review_plan(ReviewScope::whole_repository());
        let order: Vec<&str> = plan.plan.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "src/lib.rs",
                "src/main.rs",
                "src/util.rs",
                "src/a.rs",
                "src/b/mod.rs"
            ]
        );
        assert_eq!(plan.plan[2].reason, PlanReason::Hub);
        assert_eq!(plan.relevant.files, 5);
        // Tests are in scope only when asked for.
        let with_tests =
            inv.review_plan(resolve_review_scope("review the code and its tests", &inv));
        assert!(with_tests.plan.iter().any(|e| e.path == "tests/it.rs"));
    }

    #[test]
    fn scope_maps_named_directories_files_and_core() {
        let dir = rust_fixture();
        let inv = RepoInventory::scan(dir.path()).unwrap();
        let scope = resolve_review_scope("review src/b for bugs", &inv);
        assert_eq!(scope.prefixes, vec!["src/b".to_string()]);
        let plan = inv.review_plan(scope);
        assert_eq!(plan.plan.len(), 1);
        assert_eq!(
            resolve_review_scope("audit util.rs", &inv).prefixes,
            vec!["src/util.rs".to_string()]
        );
        // A bare directory basename.
        assert_eq!(
            resolve_review_scope("can you review b please", &inv)
                .prefixes
                .len(),
            0,
            "len<3 ignored"
        );
        let core = resolve_review_scope("can you review the demo core do not code", &inv);
        assert!(core.core);
        assert_eq!(
            resolve_review_scope("review this repository", &inv),
            ReviewScope::whole_repository()
        );
    }

    #[test]
    fn python_js_and_go_imports_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(r, "pkg/__init__.py", "from .core import slug\n");
        write(r, "pkg/core.py", "import re\nfrom pkg import special\n");
        write(r, "pkg/special.py", "X = 1\n");
        write(r, "pkg/__main__.py", "from pkg.core import slug\n");
        write(
            r,
            "web/src/index.ts",
            "import { a } from './lib/a';\nconst b = require('./lib/b.js');\n",
        );
        write(r, "web/src/lib/a.ts", "export const a = 1;\n");
        write(r, "web/src/lib/b.ts", "export const b = 2;\n");
        write(r, "go.mod", "module example.com/app\n\ngo 1.21\n");
        write(r, "cmd/app/main.go", "package main\n\nimport (\n\t\"fmt\"\n\t\"example.com/app/internal/store\"\n)\n\nfunc main() { fmt.Println(store.X) }\n");
        write(
            r,
            "internal/store/store.go",
            "package store\n\nconst X = 1\n",
        );
        let inv = RepoInventory::scan(r).unwrap();
        let deg = |p: &str| inv.files.iter().find(|f| f.path == p).unwrap().in_degree;
        assert_eq!(deg("pkg/core.py"), 2, "relative + absolute from-import");
        assert_eq!(deg("pkg/special.py"), 1);
        assert_eq!(deg("web/src/lib/a.ts"), 1);
        assert_eq!(deg("web/src/lib/b.ts"), 1, ".js specifier resolves to .ts");
        assert_eq!(deg("internal/store/store.go"), 1);
        assert!(inv.entry_points.contains(&"pkg/__main__.py".to_string()));
        assert!(inv.entry_points.contains(&"cmd/app/main.go".to_string()));
    }

    #[test]
    fn gitignore_is_applied_inside_a_git_work_tree() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(r)
                .output()
                .unwrap()
        };
        if !git(&["init", "-q"]).status.success() {
            return; // git unavailable: nothing to assert
        }
        write(r, ".gitignore", "ignored/\n");
        write(r, "keep.py", "x = 1\n");
        write(r, "ignored/skip.py", "y = 2\n");
        let inv = RepoInventory::scan(r).unwrap();
        assert_eq!(inv.gitignore, GitignoreStatus::Applied);
        assert!(inv.files.iter().any(|f| f.path == "keep.py"));
        assert!(inv.files.iter().all(|f| f.path != "ignored/skip.py"));
    }

    #[test]
    fn compact_rendering_is_token_bounded_and_counts_the_rest() {
        let dir = rust_fixture();
        let inv = RepoInventory::scan(dir.path()).unwrap();
        let plan = inv.review_plan(ReviewScope::whole_repository());
        let full = inv.render_compact(&plan, 10_000);
        assert!(full.contains("- src/b/mod.rs (6)"));
        let tight = inv.render_compact(&plan, 120);
        assert!(
            crate::token_count::estimate_content_tokens(&tight) <= 160,
            "{tight}"
        );
        assert!(tight.contains("more files"), "{tight}");
        let text = inv.render_text(Some(&plan));
        assert!(text.contains("most central (import in-degree"));
        assert!(text.contains("reading plan: 1. src/lib.rs [entry]"));
    }

    #[test]
    fn number_formatting() {
        assert_eq!(group(0), "0");
        assert_eq!(group(1234567), "1,234,567");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
    }
}
