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
    /// The rendered content
    pub content: String,
    /// Tokens consumed
    pub tokens_used: usize,
    /// Tokens remaining in budget
    pub tokens_remaining: usize,
    /// Coverage statistics
    pub coverage: CoverageStats,
    /// Suggestions for better usage
    pub suggestions: Vec<String>,
    /// Files included in result
    pub files_included: Vec<FileInfo>,
}

/// Coverage statistics for introspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageStats {
    pub files_total: usize,
    pub files_included: usize,
    pub coverage_pct: f64,
    pub symbols_total: usize,
    pub symbols_included: usize,
}

/// Information about an included file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub depth: String,
    pub tokens: usize,
    pub symbols: Vec<String>,
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

        // Initialize budget manager
        let mut budget = TokenBudget::new(args.max_tokens);
        // Reserve 20% for output formatting
        budget.reserve(20);

        // Determine depth - auto-select if not specified
        let depth = if let Some(d) = args.depth {
            Depth::parse(&d)?
        } else {
            budget.suggest_depth(&[])
        };

        // Collect target files (every discovered candidate is validated
        // against the same path policy before it is read).
        let files = self.collect_files(&target_path, &safety).await?;
        let total_files = files.len();

        // If we have a query, rank files by relevance
        let ranked_files: Vec<(PathBuf, f64)> = if let Some(ref query) = args.query {
            let files_with_score: Vec<(PathBuf, f64)> =
                files.into_iter().map(|f| (f, 1.0)).collect();
            let engine = CodeQueryEngine::new();
            engine.rank_files(&files_with_score, query).await
        } else {
            files.into_iter().map(|f| (f, 1.0)).collect()
        };

        // Process files within budget
        let mut files_included = Vec::new();
        let mut all_symbols = Vec::new();

        for (file_path, _relevance) in ranked_files {
            if budget.exhausted() {
                break;
            }

            // Check if we can afford this file at requested depth
            let estimate = budget.estimate_file(&file_path, &depth).await?;

            // Skip if can't afford even at minimum depth
            if estimate.min_tokens > budget.remaining() {
                continue;
            }

            // Downgrade depth if needed to fit budget
            let actual_depth = if estimate.tokens > budget.remaining() {
                budget.suggest_depth_for_file(&file_path).await?
            } else {
                depth.clone()
            };

            // Parse and extract content
            let content = tokio::fs::read_to_string(&file_path).await.ok();
            if let Some(content) = content {
                let language = Language::detect(&file_path, args.language.as_deref());
                let parsed = parser::parse(&content, language);
                let symbols = parser::extract_at_depth(&parsed, &actual_depth);

                let symbol_tokens: usize = symbols
                    .iter()
                    .map(|s| estimate_content_tokens(&s.render()))
                    .sum();

                if budget.allocate(symbol_tokens) > 0 {
                    files_included.push(FileInfo {
                        path: file_path.to_string_lossy().to_string(),
                        depth: actual_depth.as_str().to_string(),
                        tokens: symbol_tokens,
                        symbols: symbols.iter().map(|s| s.name.clone()).collect(),
                    });
                    all_symbols.extend(symbols);
                }
            }
        }

        // Generate suggestions
        let suggestions = self.generate_suggestions(
            total_files,
            files_included.len(),
            &depth,
            args.query.is_some(),
            budget.remaining(),
        );

        // Render output
        let format = args.format.as_deref().unwrap_or("tree");
        let renderer = OutputRenderer::new(format);
        let rendered = renderer.render(&files_included, &all_symbols)?;

        Ok(IntrospectResult {
            content: rendered,
            tokens_used: budget.used(),
            tokens_remaining: budget.remaining(),
            coverage: CoverageStats {
                files_total: total_files,
                files_included: files_included.len(),
                coverage_pct: (files_included.len() as f64 / total_files.max(1) as f64) * 100.0,
                symbols_total: all_symbols.len(),
                symbols_included: all_symbols.len(),
            },
            suggestions,
            files_included,
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
        total: usize,
        included: usize,
        depth: &Depth,
        has_query: bool,
        remaining: usize,
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        if included < total {
            let coverage = (included as f64 / total as f64) * 100.0;

            if coverage < 50.0 && !matches!(depth, Depth::Overview) {
                suggestions.push(format!(
                    "Only {:.0}% coverage. Use 'depth: overview' for broader coverage.",
                    coverage
                ));
            }

            if !has_query && included > 20 {
                suggestions
                    .push("Add a 'query' parameter to prioritize most relevant files.".to_string());
            }
        }

        if remaining > 2000 && included < total {
            suggestions.push(format!(
                "{} tokens remaining. Can include more files.",
                remaining
            ));
        }

        if matches!(depth, Depth::Full) && included > 5 {
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
        "Smart code reading with budget-aware depth control. \
         Depth levels: 'overview' (~50 tokens/file), 'signatures' (~200 tokens/file), \
         'full' (complete source). Automatically adjusts to fit token budget."
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
                    "description": "Level of detail: overview (metadata), signatures (API), full (complete)"
                },
                "query": {
                    "type": "string",
                    "description": "Optional search query to rank files by relevance"
                },
                "max_tokens": {
                    "type": "integer",
                    "default": 8000,
                    "description": "Maximum tokens to consume"
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

        Ok(json!({
            "query": args.query,
            "results": output,
            "total_matches": results.total_matches,
            "tokens_used": results.tokens_used,
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
