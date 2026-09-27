//! Code Introspection Tools for Selfware Evolution
//!
//! Provides context-aware code reading, semantic search, and planning tools
//! designed for limited token budgets. Enables the evolution system to
//! introspect its own codebase efficiently.
//!
//! ## Tools
//!
//! - `code_introspect`: Smart code reading with depth levels (overview/signatures/full)
//! - `code_query`: Semantic search across codebase using BM25 ranking
//! - `code_plan`: Generate budgeted execution plans for evolution tasks
//! - `code_diff_plan`: Analyze impact of code changes before mutation

pub mod budget;
pub mod parser;
pub mod planner;
pub mod query;
pub mod render;

use anyhow::Result;
use async_trait::async_trait;
use budget::{Depth, TokenBudget};
use parser::Language;
use query::CodeQueryEngine;
use render::OutputRenderer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::config::SafetyConfig;
use crate::token_count::estimate_content_tokens;
use crate::tools::file::{resolve_safety_config, validate_tool_path};
use crate::tools::Tool;

/// Result of a code introspection operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectResult {
    /// The rendered content — the only budgeted text.
    pub content: String,
    /// Tokens of `content`, measured with
    /// `crate::token_count::estimate_content_tokens` on the final text
    /// (never more than the requested `max_tokens`).
    pub tokens_used: usize,
    /// `max_tokens - tokens_used`.
    pub tokens_remaining: usize,
    /// Coverage statistics
    pub coverage: CoverageStats,
    /// Suggestions for better usage
    pub suggestions: Vec<String>,
    /// Files included in result, in relevance order
    pub files_included: Vec<FileInfo>,
}

/// Coverage statistics for introspection.
///
/// Coverage is of the OUTLINE (signatures / names at the chosen depth), not
/// of file contents: nothing here — and nothing `code_introspect` returns —
/// is a read of a file's body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageStats {
    /// Source files found under the target.
    pub files_total: usize,
    /// Files with an entry in `content` (fully or truncated).
    pub files_included: usize,
    /// Included files whose symbol list was cut short by the budget.
    #[serde(default)]
    pub files_truncated: usize,
    /// Files found but not readable as text (never included).
    #[serde(default)]
    pub files_unreadable: usize,
    /// `files_included / files_total` as a percentage.
    pub coverage_pct: f64,
    /// Symbols the chosen depth would render across ALL found files.
    pub symbols_total: usize,
    /// Symbols actually rendered in `content`.
    pub symbols_included: usize,
    /// `symbols_included / symbols_total` as a percentage (100 when there
    /// are no symbols to render).
    #[serde(default)]
    pub symbols_coverage_pct: f64,
}

impl CoverageStats {
    /// Whether anything found was left out of the output.
    pub fn is_partial(&self) -> bool {
        self.files_included < self.files_total || self.symbols_included < self.symbols_total
    }
}

/// Information about an included file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub depth: String,
    /// Tokens of this file's entry exactly as it appears in `content`.
    pub tokens: usize,
    /// Names of the symbols rendered for this file (omitted ones excluded).
    pub symbols: Vec<String>,
    /// Symbols of this file left out because the budget ran out.
    #[serde(default)]
    pub symbols_omitted: usize,
    /// The rendered line per symbol. Not serialized: the lines are in
    /// `content`, and repeating them here would double the tool output
    /// outside the budget.
    #[serde(skip)]
    pub rendered_lines: Vec<String>,
}

/// A source file that was read and parsed.
struct Candidate {
    path: PathBuf,
    parsed: parser::ParsedFile,
}

/// The result of packing candidates into the budget at one depth.
struct Packed {
    files: Vec<FileInfo>,
    content: String,
    tokens_used: usize,
    coverage: CoverageStats,
}

/// Measure a file entry's tokens as rendered. The flat view prints the
/// count inside the entry, so iterate to the fixed point (monotone, so a
/// digit-boundary oscillation settles on the larger — never under-reported —
/// value).
fn measure_block(
    renderer: &OutputRenderer,
    info: &mut FileInfo,
    index: usize,
    last_in_group: bool,
) -> usize {
    info.tokens = 0;
    for _ in 0..6 {
        let measured = estimate_content_tokens(&renderer.file_block(info, index, last_in_group));
        if measured <= info.tokens {
            break;
        }
        info.tokens = measured;
    }
    info.tokens
}

/// Re-measure every included file's entry in its final position.
fn finalize_blocks(renderer: &OutputRenderer, files: &mut [FileInfo]) {
    let flags = renderer.last_in_group_flags(files);
    for (i, file) in files.iter_mut().enumerate() {
        measure_block(renderer, file, i, flags[i]);
    }
}

fn coverage_of(
    files: &[FileInfo],
    files_total: usize,
    files_unreadable: usize,
    symbols_total: usize,
) -> CoverageStats {
    let symbols_included: usize = files.iter().map(|f| f.symbols.len()).sum();
    CoverageStats {
        files_total,
        files_included: files.len(),
        files_truncated: files.iter().filter(|f| f.symbols_omitted > 0).count(),
        files_unreadable,
        coverage_pct: (files.len() as f64 / files_total.max(1) as f64) * 100.0,
        symbols_total,
        symbols_included,
        symbols_coverage_pct: if symbols_total == 0 {
            100.0
        } else {
            (symbols_included as f64 / symbols_total as f64) * 100.0
        },
    }
}

/// The coverage line closing `content`. It states what the output is — an
/// outline — so a reader (or a review ledger) never mistakes it for a read
/// of the files.
fn coverage_footer(stats: &CoverageStats, depth: &Depth) -> String {
    let mut line = format!(
        "Coverage: {}/{} files, {}/{} symbols at depth '{}'",
        stats.files_included, stats.files_total, stats.symbols_included, stats.symbols_total, depth
    );
    if stats.files_truncated > 0 {
        line.push_str(&format!(", {} files truncated", stats.files_truncated));
    }
    if stats.files_unreadable > 0 {
        line.push_str(&format!(", {} unreadable", stats.files_unreadable));
    }
    line.push_str(
        ".\nOutline only (signatures/names, no bodies): this is not a read of the files' contents.\n",
    );
    line
}

/// Pack `candidates` (in relevance order) into `limit` tokens at `depth`.
///
/// The budget is a hard limit on the returned `content`: a file is included
/// whole if its entry fits, otherwise cut to the longest prefix of its
/// symbols whose entry (with an "N more omitted" marker) fits, otherwise
/// skipped. Every cost is measured with `estimate_content_tokens` on the
/// rendered text, and the final `content` is measured again as a whole —
/// token counts are not additive, so files are dropped from the tail until
/// the measured total fits.
fn pack(
    candidates: &[Candidate],
    depth: &Depth,
    limit: usize,
    renderer: &OutputRenderer,
    files_total: usize,
) -> Result<Packed> {
    let files_unreadable = files_total - candidates.len();
    let per_file: Vec<(Vec<String>, Vec<String>)> = candidates
        .iter()
        .map(|c| {
            let symbols = parser::extract_at_depth(&c.parsed, depth);
            let names = symbols.iter().map(|s| s.name.clone()).collect();
            let lines = symbols
                .iter()
                .map(|s| render::symbol_line(s, depth))
                .collect();
            (names, lines)
        })
        .collect();
    let symbols_total: usize = per_file.iter().map(|(n, _)| n.len()).sum();

    // The fixed frame (headers, summary, coverage footer at its widest
    // numbers) is paid first.
    let mut budget = TokenBudget::new(limit);
    let widest = coverage_of(&[], files_total, files_unreadable, symbols_total);
    let widest = CoverageStats {
        files_included: files_total,
        files_truncated: files_total,
        symbols_included: symbols_total,
        ..widest
    };
    let frame =
        estimate_content_tokens(&(renderer.render(&[])? + &coverage_footer(&widest, depth)));
    if !budget.try_allocate(frame) {
        anyhow::bail!(
            "max_tokens {limit} is below the fixed cost of the result frame ({frame} tokens); \
             raise max_tokens"
        );
    }

    let mut included: Vec<FileInfo> = Vec::new();
    let mut groups_opened = std::collections::HashSet::new();
    for (cand, (names, lines)) in candidates.iter().zip(per_file) {
        let index = included.len();
        let mut info = FileInfo {
            path: cand.path.to_string_lossy().to_string(),
            depth: depth.as_str().to_string(),
            tokens: 0,
            symbols: names.clone(),
            symbols_omitted: 0,
            rendered_lines: lines.clone(),
        };
        // A file opening a new directory group (tree view) also pays for
        // that group's header line.
        let header = renderer
            .group_header(&info)
            .filter(|h| !groups_opened.contains(h));
        let header_cost = header.as_deref().map_or(0, estimate_content_tokens);
        if header_cost > budget.remaining() {
            continue;
        }
        let full = measure_block(renderer, &mut info, index, true);
        if budget.try_allocate(full + header_cost) {
            groups_opened.extend(header);
            included.push(info);
            continue;
        }

        // Truncate: the longest prefix of symbols whose entry fits.
        let n = lines.len();
        let mut best = None;
        if n >= 2 {
            let (mut lo, mut hi) = (1usize, n - 1);
            while lo <= hi {
                let mid = lo + (hi - lo) / 2;
                info.symbols = names[..mid].to_vec();
                info.rendered_lines = lines[..mid].to_vec();
                info.symbols_omitted = n - mid;
                if measure_block(renderer, &mut info, index, true) + header_cost
                    <= budget.remaining()
                {
                    best = Some(mid);
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            }
        }
        if let Some(k) = best {
            info.symbols = names[..k].to_vec();
            info.rendered_lines = lines[..k].to_vec();
            info.symbols_omitted = n - k;
            let cost = measure_block(renderer, &mut info, index, true);
            if budget.try_allocate(cost + header_cost) {
                groups_opened.extend(header);
                included.push(info);
            }
        }
    }

    // Final measurement of the whole content; the hard limit is enforced
    // on what is actually returned.
    loop {
        finalize_blocks(renderer, &mut included);
        let coverage = coverage_of(&included, files_total, files_unreadable, symbols_total);
        let content = renderer.render(&included)? + &coverage_footer(&coverage, depth);
        let tokens_used = estimate_content_tokens(&content);
        if tokens_used <= limit {
            return Ok(Packed {
                files: included,
                content,
                tokens_used,
                coverage,
            });
        }
        // Over by a few tokens (counts are not additive): shed the tail
        // one symbol at a time, then whole files.
        if let Some(last) = included.last_mut() {
            if last.rendered_lines.len() >= 2 {
                last.rendered_lines.pop();
                last.symbols.pop();
                last.symbols_omitted += 1;
                continue;
            }
        }
        if included.pop().is_none() {
            anyhow::bail!(
                "max_tokens {limit} is below the measured cost of an empty result \
                 ({tokens_used} tokens); raise max_tokens"
            );
        }
    }
}

// ============================================================================
// Code Introspect Tool
// ============================================================================

/// Primary introspection tool - smart code reading with budget awareness
#[derive(Default)]
pub struct CodeIntrospect {
    /// Per-instance safety config for path-policy enforcement; falls back to
    /// the process-global config when `None`.
    pub safety_config: Option<SafetyConfig>,
}

impl CodeIntrospect {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the tool with an explicit safety config.
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }

    async fn execute_internal(&self, args: Value) -> Result<IntrospectResult> {
        #[derive(Deserialize)]
        struct Args {
            target: String,
            #[serde(default)]
            depth: Option<String>,
            #[serde(default)]
            query: Option<String>,
            #[serde(default = "default_max_tokens")]
            max_tokens: usize,
            #[serde(default)]
            format: Option<String>,
            #[serde(default)]
            language: Option<String>,
        }

        fn default_max_tokens() -> usize {
            8000
        }

        let args: Args = serde_json::from_value(args)?;
        let target_path = PathBuf::from(&args.target);

        // The tool WALKS and READS every source file under `target` — the
        // root must obey the same workspace path policy as file_read
        // (2026-09-21 review sweep).
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.target, &safety)?;

        let format = args.format.as_deref().unwrap_or("tree");
        let renderer = OutputRenderer::new(format);
        let limit = args.max_tokens;

        // 1. Collect every candidate first (validated against the path
        //    policy before it is read); sorted so the order without a query
        //    is deterministic rather than directory-listing order.
        let mut files = self.collect_files(&target_path, &safety).await?;
        files.sort();
        let files_total = files.len();

        // 2. Rank by the query. `rank_files` recomputes a BM25 score per file
        //    from the query terms against the parsed symbols (the passed
        //    base score, a constant 1.0, is only a multiplier), so the query
        //    does reorder. The index is built over the candidates first so
        //    IDF and length normalisation reflect this corpus — without it
        //    every term had the same IDF and raw byte length was divided by
        //    an average of 1.
        let ordered: Vec<PathBuf> = if let Some(ref query) = args.query {
            let mut engine = CodeQueryEngine::new();
            engine.build_index(&files).await?;
            let with_score: Vec<(PathBuf, f64)> = files.into_iter().map(|f| (f, 1.0)).collect();
            engine
                .rank_files(&with_score, query)
                .await
                .into_iter()
                .map(|(p, _)| p)
                .collect()
        } else {
            files
        };

        // 3. Read and parse every candidate once.
        let mut candidates = Vec::with_capacity(ordered.len());
        for path in ordered {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let language = Language::detect(&path, args.language.as_deref());
                let parsed = parser::parse(&content, language);
                candidates.push(Candidate { path, parsed });
            }
        }

        // 4. Depth: explicit, or chosen from measured packings of the
        //    collected set (most detailed depth that covers everything, else
        //    the one that reaches the most files).
        let (depth, packed) = if let Some(d) = args.depth {
            let depth = Depth::parse(&d)?;
            let packed = pack(&candidates, &depth, limit, &renderer, files_total)?;
            (depth, packed)
        } else {
            let mut options = Vec::new();
            let mut packings = Vec::new();
            for depth in [Depth::Signatures, Depth::Overview] {
                let packed = pack(&candidates, &depth, limit, &renderer, files_total)?;
                options.push(budget::MeasuredDepth {
                    depth: depth.clone(),
                    // Complete = every READABLE file with all its symbols
                    // (unreadable files are partial at every depth).
                    complete: packed.coverage.files_included
                        == files_total - packed.coverage.files_unreadable
                        && packed.coverage.symbols_included == packed.coverage.symbols_total,
                    files_included: packed.coverage.files_included,
                    symbols_total: packed.coverage.symbols_total,
                    symbols_included: packed.coverage.symbols_included,
                });
                packings.push((depth, packed));
            }
            let chosen = TokenBudget::suggest_depth(&options).unwrap_or(Depth::Signatures);
            let idx = packings.iter().position(|(d, _)| *d == chosen).unwrap_or(0);
            packings.swap_remove(idx)
        };

        let suggestions = self.generate_suggestions(&packed.coverage, &depth, args.query.is_some());

        Ok(IntrospectResult {
            content: packed.content,
            tokens_used: packed.tokens_used,
            tokens_remaining: limit - packed.tokens_used,
            coverage: packed.coverage,
            suggestions,
            files_included: packed.files,
        })
    }

    async fn collect_files(&self, target: &Path, safety: &SafetyConfig) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if target.is_file() {
            files.push(target.to_path_buf());
        } else if target.is_dir() {
            let mut entries = tokio::fs::read_dir(target).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                // Every candidate that the walk will actually READ is
                // validated against the same workspace path policy as the
                // target itself (containment + symlink resolution, via
                // `validate_tool_path`) BEFORE the read happens
                // (2026-09-21 review P2: only the ROOT target was
                // validated; a denied source file nested inside an allowed
                // directory, or a symlink escaping the workspace, was read
                // unvalidated). Directories are validated before descent so
                // an escaping symlink directory is refused at the boundary;
                // non-source files are never read and need no validation
                // (skipping them keeps a `.env.example` from breaking the
                // walk).
                if path.is_file() && Self::is_source_file(&path) {
                    validate_tool_path(&path.to_string_lossy(), safety)?;
                    files.push(path);
                } else if path.is_dir() {
                    validate_tool_path(&path.to_string_lossy(), safety)?;
                    // Recursively collect with depth limit
                    files.extend(self.collect_files_recursive(&path, 3, safety).await?);
                }
            }
        }

        Ok(files)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn collect_files_recursive<'a>(
        &'a self,
        dir: &'a Path,
        depth: usize,
        safety: &'a SafetyConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PathBuf>>> + Send + 'a>>
    {
        Box::pin(async move {
            if depth == 0 {
                return Ok(Vec::new());
            }

            let mut files = Vec::new();
            let mut entries = tokio::fs::read_dir(dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                // Skip common non-source directories
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy();
                    if matches!(
                        name.as_ref(),
                        "target" | "node_modules" | ".git" | "__pycache__" | ".venv" | "scratchpad"
                    ) {
                        continue;
                    }
                }

                // Validate each candidate the walk will read, exactly as in
                // `collect_files` (see the comment there).
                if path.is_file() && Self::is_source_file(&path) {
                    validate_tool_path(&path.to_string_lossy(), safety)?;
                    files.push(path);
                } else if path.is_dir() {
                    validate_tool_path(&path.to_string_lossy(), safety)?;
                    files.extend(
                        self.collect_files_recursive(&path, depth - 1, safety)
                            .await?,
                    );
                }
            }

            Ok(files)
        })
    }

    fn is_source_file(path: &Path) -> bool {
        let extensions = ["rs", "py", "js", "ts", "go", "java", "c", "cpp", "h", "hpp"];
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| extensions.contains(&e))
            .unwrap_or(false)
    }

    fn generate_suggestions(
        &self,
        coverage: &CoverageStats,
        depth: &Depth,
        has_query: bool,
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Any partial coverage is named — how much, and what was left out —
        // at every depth, overview included (the old gate warned only below
        // 50% file coverage and never at overview).
        if coverage.is_partial() {
            let readable = coverage.files_total - coverage.files_unreadable;
            let skipped = readable.saturating_sub(coverage.files_included);
            let mut left_out = Vec::new();
            if skipped > 0 {
                left_out.push(format!("{skipped} files did not fit the budget"));
            }
            if coverage.files_truncated > 0 {
                left_out.push(format!("{} files were truncated", coverage.files_truncated));
            }
            if coverage.files_unreadable > 0 {
                left_out.push(format!(
                    "{} files could not be read",
                    coverage.files_unreadable
                ));
            }
            let omitted = coverage.symbols_total - coverage.symbols_included;
            if omitted > 0 {
                left_out.push(format!("{omitted} symbols omitted"));
            }
            let remedy = if matches!(depth, Depth::Overview) {
                "Raise max_tokens or narrow the target."
            } else {
                "Raise max_tokens, narrow the target, or use 'depth: overview'."
            };
            suggestions.push(format!(
                "Partial coverage: {}/{} files ({:.0}%), {}/{} symbols ({:.0}%) at depth '{}' — {}. {}",
                coverage.files_included,
                coverage.files_total,
                coverage.coverage_pct,
                coverage.symbols_included,
                coverage.symbols_total,
                coverage.symbols_coverage_pct,
                depth,
                left_out.join(", "),
                remedy
            ));

            if !has_query && coverage.files_included > 20 {
                suggestions
                    .push("Add a 'query' parameter to prioritize most relevant files.".to_string());
            }
        }

        if matches!(depth, Depth::Full) && coverage.files_included > 5 {
            suggestions.push(
                "Using 'depth: full' on many files. Consider 'signatures' for better coverage."
                    .to_string(),
            );
        }

        suggestions
    }
}

#[async_trait]
impl Tool for CodeIntrospect {
    fn name(&self) -> &str {
        "code_introspect"
    }

    fn description(&self) -> &str {
        "Budget-bounded code outline (signatures and symbol names, never function bodies). \
         Depth levels: 'overview' (imports + one line per symbol), 'signatures' (public API \
         signatures), 'full' (every symbol's signature, private included). max_tokens is a \
         hard limit on the returned content; when it cannot hold everything the result \
         reports partial coverage. Not a substitute for file_read."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Path to file or directory to introspect"
                },
                "depth": {
                    "type": "string",
                    "enum": ["overview", "signatures", "full", "dependencies"],
                    "description": "Level of detail: overview (imports + symbol list), signatures (public API), full (all signatures). Omit to choose from measured sizes."
                },
                "query": {
                    "type": "string",
                    "description": "Optional search query to rank files by relevance"
                },
                "max_tokens": {
                    "type": "integer",
                    "default": 8000,
                    "description": "Hard limit on the tokens of the returned content"
                },
                "format": {
                    "type": "string",
                    "enum": ["tree", "flat", "graph"],
                    "default": "tree",
                    "description": "Output format"
                },
                "language": {
                    "type": "string",
                    "enum": ["auto", "rust", "python", "typescript"],
                    "default": "auto",
                    "description": "Language for parsing (auto-detected if not specified)"
                }
            },
            "required": ["target"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let result = self.execute_internal(args).await?;
        Ok(serde_json::to_value(result)?)
    }
}

// ============================================================================
// Code Query Tool
// ============================================================================

/// Semantic code query tool
#[derive(Default)]
pub struct CodeQuery {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl CodeQuery {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the tool with an explicit safety config.
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for CodeQuery {
    fn name(&self) -> &str {
        "code_query"
    }

    fn description(&self) -> &str {
        "Semantic search across codebase using BM25 ranking. \
         Finds code by meaning, not just exact text match. \
         Returns most relevant symbols with file paths and line numbers."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language query describing what to find"
                },
                "scope": {
                    "type": "string",
                    "description": "Directory scope to search (default: current directory)"
                },
                "max_results": {
                    "type": "integer",
                    "default": 10,
                    "description": "Maximum number of results to return"
                },
                "include_bodies": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include full function bodies in results"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Args {
            query: String,
            #[serde(default)]
            scope: Option<String>,
            #[serde(default = "default_max_results")]
            max_results: usize,
            #[serde(default)]
            include_bodies: bool,
        }

        fn default_max_results() -> usize {
            10
        }

        let args: Args = serde_json::from_value(args)?;
        let scope = args.scope.unwrap_or_else(|| ".".to_string());
        let scope_path = PathBuf::from(&scope);

        // code_query WALKS and READS every source file under `scope` — the
        // scope root must obey the same workspace path policy as file_read
        // (2026-09-21 review: `scope` was forwarded to a recursive walk
        // unvalidated, unlike the checker's `path`-key checks).
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&scope, &safety)?;

        // Collect files in scope (every discovered candidate is validated
        // against the same path policy before it is read).
        let mut files = Vec::new();
        Self::collect_files(&scope_path, &mut files, &safety).await?;
        // Deterministic tie order for equal relevance.
        files.sort();

        // Build query engine and search
        let mut engine = CodeQueryEngine::new();
        engine.build_index(&files).await?;

        let budget = TokenBudget::new(4000);
        let results = engine.search(&args.query, &files, &budget).await?;

        // Format results
        let mut output = Vec::new();
        for result in results.results.iter().take(args.max_results) {
            for symbol in &result.matched_symbols {
                output.push(json!({
                    "file": result.path.to_string_lossy().to_string(),
                    "name": symbol.name,
                    "kind": format!("{:?}", symbol.kind),
                    "signature": symbol.signature,
                    "line": symbol.line_start,
                }));
            }
        }

        // `tokens_used` is measured on the results actually returned (after
        // `max_results`), not on the packed set before it was cut.
        let returned_files = results.results.len().min(args.max_results);
        let tokens_used = estimate_content_tokens(&serde_json::to_string(&output)?);

        Ok(json!({
            "query": args.query,
            "results": output,
            "total_matches": results.total_matches,
            "files_returned": returned_files,
            "tokens_used": tokens_used,
        }))
    }
}

impl CodeQuery {
    async fn collect_files(
        dir: &Path,
        files: &mut Vec<PathBuf>,
        safety: &SafetyConfig,
    ) -> Result<()> {
        if !dir.is_dir() {
            if dir.is_file() && CodeIntrospect::is_source_file(dir) {
                validate_tool_path(&dir.to_string_lossy(), safety)?;
                files.push(dir.to_path_buf());
            }
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                if matches!(
                    name.as_ref(),
                    "target" | "node_modules" | ".git" | "__pycache__" | "scratchpad"
                ) {
                    continue;
                }
            }

            // Validate each candidate the walk will read (see the
            // `CodeIntrospect::collect_files` comment for the rationale).
            if path.is_file() && CodeIntrospect::is_source_file(&path) {
                validate_tool_path(&path.to_string_lossy(), safety)?;
                files.push(path);
            } else if path.is_dir() {
                validate_tool_path(&path.to_string_lossy(), safety)?;
                Box::pin(Self::collect_files(&path, files, safety)).await?;
            }
        }

        Ok(())
    }
}

// ============================================================================
// Code Plan Tool
// ============================================================================

/// Evolution planning tool
#[derive(Default)]
pub struct CodePlan {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl CodePlan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the tool with an explicit safety config.
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for CodePlan {
    fn name(&self) -> &str {
        "code_plan"
    }

    fn description(&self) -> &str {
        "Generate a structured execution plan for evolution tasks. \
         Breaks down goals into phases with specific actions, \
         respecting token and iteration budgets. Returns a plan with \
         estimated costs and risk assessment."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "Description of what to achieve"
                },
                "budget_iterations": {
                    "type": "integer",
                    "default": 20,
                    "description": "Maximum iterations allowed"
                },
                "budget_tokens": {
                    "type": "integer",
                    "default": 100000,
                    "description": "Maximum tokens to use"
                },
                "strategy": {
                    "type": "string",
                    "enum": ["breadth_first", "depth_first", "impact_analysis"],
                    "description": "Planning strategy"
                },
                "codebase_root": {
                    "type": "string",
                    "default": ".",
                    "description": "Root directory of codebase"
                }
            },
            "required": ["goal"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        use planner::EvolutionPlanner;

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Args {
            goal: String,
            #[serde(default = "default_iterations")]
            budget_iterations: usize,
            #[serde(default = "default_tokens")]
            budget_tokens: usize,
            #[serde(default)]
            strategy: Option<String>,
            #[serde(default = "default_root")]
            codebase_root: String,
        }

        fn default_iterations() -> usize {
            20
        }
        fn default_tokens() -> usize {
            100000
        }
        fn default_root() -> String {
            ".".to_string()
        }

        let args: Args = serde_json::from_value(args)?;
        let root = PathBuf::from(&args.codebase_root);

        // code_plan WALKS and READS the codebase under `codebase_root` — the
        // root must obey the same workspace path policy as file_read
        // (2026-09-21 review: `codebase_root` was forwarded to the planner
        // unvalidated).
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.codebase_root, &safety)?;

        let planner = EvolutionPlanner::new(
            args.goal.clone(),
            args.budget_iterations,
            args.budget_tokens,
            root,
        );

        let plan = planner.generate_plan().await?;

        Ok(serde_json::to_value(plan)?)
    }
}

// ============================================================================
// Code Diff Plan Tool
// ============================================================================

/// Change impact analysis tool
#[derive(Default)]
pub struct CodeDiffPlan {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl CodeDiffPlan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the tool with an explicit safety config.
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for CodeDiffPlan {
    fn name(&self) -> &str {
        "code_diff_plan"
    }

    fn description(&self) -> &str {
        "Analyze the impact of a code change before mutation. \
         Finds direct callers, transitive dependencies, and affected tests. \
         Returns a suggested order of operations for safe changes."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target_file": {
                    "type": "string",
                    "description": "File that will be modified"
                },
                "change_type": {
                    "type": "string",
                    "enum": ["modify", "delete", "rename"],
                    "description": "Type of change being made"
                },
                "affected_symbol": {
                    "type": "string",
                    "description": "Specific function/struct being changed (optional)"
                },
                "codebase_root": {
                    "type": "string",
                    "default": ".",
                    "description": "Root directory of codebase"
                }
            },
            "required": ["target_file", "change_type"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        use planner::analyze_impact;

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Args {
            target_file: String,
            change_type: String,
            #[serde(default)]
            affected_symbol: Option<String>,
            #[serde(default = "default_root")]
            codebase_root: String,
        }

        fn default_root() -> String {
            ".".to_string()
        }

        let args: Args = serde_json::from_value(args)?;
        let target = PathBuf::from(&args.target_file);
        let root = PathBuf::from(&args.codebase_root);

        // code_diff_plan READS the target file and walks the codebase under
        // `codebase_root` for impact analysis — both must obey the same
        // workspace path policy as file_read (2026-09-21 review sweep; the
        // checker's introspection arm already covers these keys).
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.target_file, &safety)?;
        validate_tool_path(&args.codebase_root, &safety)?;

        let analysis = analyze_impact(&target, args.affected_symbol.as_deref(), &root).await?;

        Ok(serde_json::to_value(analysis)?)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/tools/introspect/mod_test.rs"]
mod tests;
