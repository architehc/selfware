use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tracing::instrument;
use walkdir::WalkDir;

use crate::tools::grep_search::cached_regex;
#[cfg(test)]
use crate::tools::grep_search::MAX_PATTERN_LENGTH;

/// Re-export the canonical `grep_search` tool from `tools::grep_search` so
/// there is only one `GrepSearch` implementation in the codebase.
pub use crate::tools::grep_search::{GrepMatch, GrepSearch, GrepSearchResult};

/// Finds files by glob pattern (e.g. `**/*.rs`), returning paths with metadata.
pub struct GlobFind;
/// Finds Rust symbol definitions (functions, structs, enums, traits, etc.) by name.
pub struct SymbolSearch;

/// Result of a glob find operation
#[derive(Debug, Serialize, Deserialize)]
struct FileInfo {
    path: String,
    size: u64,
    modified: Option<String>,
}

/// A symbol found in code
#[derive(Debug, Serialize, Deserialize)]
struct Symbol {
    name: String,
    symbol_type: String,
    file: String,
    line: u32,
    signature: String,
}

#[async_trait]
impl Tool for GlobFind {
    fn name(&self) -> &str {
        "glob_find"
    }

    fn description(&self) -> &str {
        "Find files by glob pattern (e.g., *.rs, src/**/*.ts). Returns file paths with metadata. Use to locate files before reading or editing."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., *.rs, src/**/*.ts, **/*_test.go)"
                },
                "path": {
                    "type": "string",
                    "default": ".",
                    "description": "Base directory to search from"
                },
                "max_results": {
                    "type": "integer",
                    "default": 100,
                    "description": "Maximum results to return"
                }
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let result = tokio::task::spawn_blocking(move || -> Result<Value> {
            let pattern_str = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .context("Missing required parameter: pattern")?;

            let base_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as usize;

            // Combine base path with pattern. Patterns that are absolute, already
            // relative to the current directory, or recursive-from-root (`**`)
            // are used as-is.
            let full_pattern = if pattern_str.starts_with('/')
                || pattern_str.starts_with("./")
                || pattern_str.starts_with("**")
            {
                pattern_str.to_string()
            } else {
                format!("{}/{}", base_path, pattern_str)
            };

            // WalkDir may prefix paths with `./` when the base path is `.`.
            // Strip that from both the pattern and the path so matching is
            // independent of how the base path was expressed.
            let match_pattern = full_pattern.strip_prefix("./").unwrap_or(&full_pattern);

            let glob_pattern = glob::Pattern::new(match_pattern).context("Invalid glob pattern")?;

            // `*` should match within a single path component, while `**` should
            // match across directory boundaries. The default `glob::Pattern`
            // lets `*` cross `/`, so we require literal separators.
            let glob_options = glob::MatchOptions {
                case_sensitive: true,
                require_literal_separator: true,
                require_literal_leading_dot: false,
            };

            let mut files = Vec::new();

            // Walk directory and match against pattern
            for entry in WalkDir::new(base_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                if files.len() >= max_results {
                    break;
                }

                let path = entry.path();
                let path_str = path.to_string_lossy();

                // Skip common directories
                if path_str.contains("/.git/")
                    || path_str.contains("/target/")
                    || path_str.contains("/node_modules/")
                {
                    continue;
                }

                let match_path = path_str.strip_prefix("./").unwrap_or(&path_str);

                if glob_pattern.matches_with(match_path, glob_options) {
                    let metadata = std::fs::metadata(path).ok();
                    let modified = metadata.as_ref().and_then(|m| {
                        m.modified().ok().map(|t| {
                            let datetime: chrono::DateTime<chrono::Utc> = t.into();
                            datetime.to_rfc3339()
                        })
                    });

                    files.push(FileInfo {
                        path: path_str.to_string(),
                        size: metadata.map(|m| m.len()).unwrap_or(0),
                        modified,
                    });
                }
            }

            let truncated = files.len() >= max_results;

            Ok(serde_json::json!({
                "files": files,
                "count": files.len(),
                "truncated": truncated
            }))
        })
        .await??;
        Ok(result)
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

#[async_trait]
impl Tool for SymbolSearch {
    fn name(&self) -> &str {
        "symbol_search"
    }

    fn description(&self) -> &str {
        "Find function, struct, enum, trait, or impl definitions in Rust code. Use to locate code symbols for navigation or understanding."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Symbol name or pattern to search for"
                },
                "path": {
                    "type": "string",
                    "default": ".",
                    "description": "Directory to search in"
                },
                "symbol_type": {
                    "type": "string",
                    "enum": ["function", "struct", "enum", "trait", "impl", "const", "type", "mod", "all"],
                    "default": "all",
                    "description": "Type of symbol to search for"
                },
                "max_results": {
                    "type": "integer",
                    "default": 50,
                    "description": "Maximum results to return"
                }
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let result = tokio::task::spawn_blocking(move || -> Result<Value> {
            let name_pattern = args
                .get("name")
                .and_then(|v| v.as_str())
                .context("Missing required parameter: name")?;

            let base_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

            let symbol_type = args
                .get("symbol_type")
                .and_then(|v| v.as_str())
                .unwrap_or("all");

            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as usize;

            // Build patterns for different symbol types
            let patterns = build_symbol_patterns(symbol_type, name_pattern)?;

            let mut symbols = Vec::new();

            // Walk Rust files
            for entry in WalkDir::new(base_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().is_file()
                        && e.path().extension().map(|ext| ext == "rs").unwrap_or(false)
                })
            {
                if symbols.len() >= max_results {
                    break;
                }

                let path = entry.path();
                let path_str = path.to_string_lossy();

                // Skip target directory
                if path_str.contains("/target/") {
                    continue;
                }

                // Wrap blocking file I/O in block_in_place to avoid blocking the async runtime
                let content = match tokio::task::block_in_place(|| std::fs::read_to_string(path)) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                for (regex, sym_type) in &patterns {
                    if symbols.len() >= max_results {
                        break;
                    }

                    for (line_num, line) in content.lines().enumerate() {
                        if symbols.len() >= max_results {
                            break;
                        }

                        if let Some(caps) = regex.captures(line) {
                            let symbol_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");

                            // Verify the name matches our pattern
                            if !symbol_name
                                .to_lowercase()
                                .contains(&name_pattern.to_lowercase())
                            {
                                continue;
                            }

                            symbols.push(Symbol {
                                name: symbol_name.to_string(),
                                symbol_type: sym_type.to_string(),
                                file: path_str.to_string(),
                                line: (line_num + 1) as u32,
                                signature: line.trim().to_string(),
                            });
                        }
                    }
                }
            }

            Ok(serde_json::json!({
                "symbols": symbols,
                "count": symbols.len()
            }))
        })
        .await??;
        Ok(result)
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// Pre-compiled symbol regexes (compiled once, reused forever).
struct SymbolRegexes {
    fn_pattern: Regex,
    struct_pattern: Regex,
    enum_pattern: Regex,
    trait_pattern: Regex,
    impl_pattern: Regex,
    const_pattern: Regex,
    type_pattern: Regex,
    mod_pattern: Regex,
}

static SYMBOL_REGEXES: Lazy<SymbolRegexes> = Lazy::new(|| SymbolRegexes {
    fn_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)").unwrap(),
    struct_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?struct\s+(\w+)").unwrap(),
    enum_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?enum\s+(\w+)").unwrap(),
    trait_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?trait\s+(\w+)").unwrap(),
    impl_pattern: Regex::new(r"impl(?:<[^>]*>)?\s+(?:(\w+)|(?:\w+\s+for\s+(\w+)))").unwrap(),
    const_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?const\s+(\w+)").unwrap(),
    type_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?type\s+(\w+)").unwrap(),
    mod_pattern: Regex::new(r"(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+(\w+)").unwrap(),
});

/// Build regex patterns for different Rust symbol types.
/// The underlying regexes are compiled once via `Lazy` statics.
fn build_symbol_patterns(
    symbol_type: &str,
    _name_pattern: &str,
) -> Result<Vec<(&'static Regex, &'static str)>> {
    let sr = &*SYMBOL_REGEXES;
    let mut patterns = Vec::new();

    match symbol_type {
        "function" => patterns.push((&sr.fn_pattern, "function")),
        "struct" => patterns.push((&sr.struct_pattern, "struct")),
        "enum" => patterns.push((&sr.enum_pattern, "enum")),
        "trait" => patterns.push((&sr.trait_pattern, "trait")),
        "impl" => patterns.push((&sr.impl_pattern, "impl")),
        "const" => patterns.push((&sr.const_pattern, "const")),
        "type" => patterns.push((&sr.type_pattern, "type")),
        "mod" => patterns.push((&sr.mod_pattern, "mod")),
        _ => {
            patterns.push((&sr.fn_pattern, "function"));
            patterns.push((&sr.struct_pattern, "struct"));
            patterns.push((&sr.enum_pattern, "enum"));
            patterns.push((&sr.trait_pattern, "trait"));
            patterns.push((&sr.impl_pattern, "impl"));
            patterns.push((&sr.const_pattern, "const"));
            patterns.push((&sr.type_pattern, "type"));
            patterns.push((&sr.mod_pattern, "mod"));
        }
    }

    Ok(patterns)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
#[path = "../../tests/unit/tools/search/search_test.rs"]
mod tests;

/// Standalone function for grep search (for use in tests and other modules)
///
/// # Arguments
/// * `pattern` - Regex pattern to search for
/// * `path` - File or directory to search in
/// * `recursive` - Whether to search recursively
/// * `max_matches` - Maximum number of matches to return
/// * `offset` - Number of matches to skip
///
/// # Returns
/// A GrepSearchResult containing matches and metadata
pub fn grep_search(
    pattern: &str,
    path: &str,
    recursive: bool,
    max_matches: usize,
    offset: usize,
) -> GrepSearchResult {
    let mut matches = Vec::new();
    let mut file_count = 0;
    let re = cached_regex(pattern).unwrap_or_else(|_| {
        // Fallback to simple regex if caching fails
        regex::Regex::new(pattern).expect("Invalid regex pattern")
    });

    if Path::new(path).is_file() {
        // Search single file
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let lines: Vec<&str> = content.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                let line_num = (line_idx + 1) as u32;
                let column = line.find(re.as_str()).map_or(1, |i| i + 1) as u32;

                matches.push(GrepMatch {
                    file: path.to_string(),
                    line: line_num,
                    column,
                    content: line.to_string(),
                    context_before: Vec::new(),
                    context_after: Vec::new(),
                });
            }
        }
        file_count = 1;
    } else if recursive {
        // Search directory recursively
        for entry in WalkDir::new(path).into_iter().flatten() {
            if entry.file_type().is_file() {
                let path_str = entry.path().to_string_lossy();
                let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
                let lines: Vec<&str> = content.lines().collect();

                for (line_idx, line) in lines.iter().enumerate() {
                    if re.is_match(line) {
                        let line_num = (line_idx + 1) as u32;
                        let column = line.find(re.as_str()).map_or(1, |i| i + 1) as u32;

                        matches.push(GrepMatch {
                            file: path_str.to_string(),
                            line: line_num,
                            column,
                            content: line.to_string(),
                            context_before: Vec::new(),
                            context_after: Vec::new(),
                        });
                    }
                }
                file_count += 1;
            }
        }
    }

    let total_matches = matches.len();
    // Apply offset and limit
    matches = matches.into_iter().skip(offset).take(max_matches).collect();

    GrepSearchResult {
        matches,
        total_matches,
        file_count,
    }
}
