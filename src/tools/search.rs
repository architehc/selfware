use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tracing::instrument;
use walkdir::WalkDir;

/// Maximum number of compiled regex patterns to cache.
const REGEX_CACHE_MAX: usize = 64;

/// Maximum length of a user-supplied regex pattern (bytes).
/// Prevents denial-of-service via extremely large or complex patterns.
const MAX_PATTERN_LENGTH: usize = 1_000;

/// Maximum compiled regex size (bytes). Limits memory consumed by
/// pathological patterns that expand into large automata.
const MAX_REGEX_SIZE: usize = 1 << 20; // 1 MB

/// Global cache of compiled regex patterns, keyed by their source string.
/// When the cache exceeds [`REGEX_CACHE_MAX`] entries, it is cleared entirely
/// (simple but bounded eviction strategy).
static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Return a cached `Regex` for `pattern`, compiling and caching it on first use.
///
/// Applies size limits to prevent ReDoS attacks from pathological patterns.
fn cached_regex(pattern: &str) -> Result<Regex> {
    if pattern.len() > MAX_PATTERN_LENGTH {
        anyhow::bail!(
            "Regex pattern too long ({} bytes, max {})",
            pattern.len(),
            MAX_PATTERN_LENGTH
        );
    }

    let mut cache = REGEX_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(re) = cache.get(pattern) {
        return Ok(re.clone());
    }

    let re = RegexBuilder::new(pattern)
        .size_limit(MAX_REGEX_SIZE)
        .build()
        .context("Invalid or too-complex regex pattern")?;

    // Evict all entries when the cache is full to stay bounded.
    if cache.len() >= REGEX_CACHE_MAX {
        cache.clear();
    }

    cache.insert(pattern.to_owned(), re.clone());
    Ok(re)
}

/// Searches file contents for regex patterns, returning matching lines with context.
pub struct GrepSearch;
/// Finds files by glob pattern (e.g. `**/*.rs`), returning paths with metadata.
pub struct GlobFind;
/// Finds Rust symbol definitions (functions, structs, enums, traits, etc.) by name.
pub struct SymbolSearch;

/// A single match result from grep search
#[derive(Debug, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Result of a grep search operation
#[derive(Debug, Serialize, Deserialize)]
pub struct GrepSearchResult {
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
    pub file_count: usize,
}

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
impl Tool for GrepSearch {
    fn name(&self) -> &str {
        "grep_search"
    }

    fn description(&self) -> &str {
        "Search for regex patterns in files. Returns matching lines with context. Use for finding code patterns, error messages, or specific text."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["pattern", "path"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in"
                },
                "recursive": {
                    "type": "boolean",
                    "default": true,
                    "description": "Search directories recursively"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "default": false,
                    "description": "Ignore case when matching"
                },
                "context_lines": {
                    "type": "integer",
                    "default": 2,
                    "description": "Lines of context before and after match"
                },
                "max_matches": {
                    "type": "integer",
                    "default": 100,
                    "description": "Maximum matches to return"
                },
                "offset": {
                    "type": "integer",
                    "default": 0,
                    "description": "Number of matches to skip (for pagination)"
                },
                "include": {
                    "type": "string",
                    "description": "Only search files matching this glob pattern (e.g., *.rs)"
                },
                "exclude": {
                    "type": "string",
                    "description": "Exclude files matching this glob pattern"
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

            let path_str = args
                .get("path")
                .and_then(|v| v.as_str())
                .context("Missing required parameter: path")?;

            let recursive = args
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let case_insensitive = args
                .get("case_insensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let context_lines = args
                .get("context_lines")
                .and_then(|v| v.as_u64())
                .unwrap_or(2) as usize;
            let max_matches = args
                .get("max_matches")
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as usize;
            let skip_offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let include_pattern = args.get("include").and_then(|v| v.as_str());
            let exclude_pattern = args.get("exclude").and_then(|v| v.as_str());

            // Build regex (uses a bounded cache to avoid recompilation)
            let full_pattern = if case_insensitive {
                format!("(?i){}", pattern_str)
            } else {
                pattern_str.to_string()
            };
            let regex = cached_regex(&full_pattern)?;

            // Build include/exclude globs
            let include_glob = include_pattern
                .map(glob::Pattern::new)
                .transpose()
                .context("Invalid include pattern")?;
            let exclude_glob = exclude_pattern
                .map(glob::Pattern::new)
                .transpose()
                .context("Invalid exclude pattern")?;

            let path = Path::new(path_str);
            let mut matches = Vec::new();
            let mut total_matches = 0;

            // Collect files to search
            let files: Vec<_> = if path.is_file() {
                vec![path.to_path_buf()]
            } else {
                let walker = if recursive {
                    WalkDir::new(path)
                } else {
                    WalkDir::new(path).max_depth(1)
                };

                walker
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .filter(|e| {
                        let file_name = e.file_name().to_string_lossy();
                        // Skip hidden files and common binary/build directories
                        if file_name.starts_with('.') {
                            return false;
                        }
                        let path_str = e.path().to_string_lossy();
                        if path_str.contains("/target/")
                            || path_str.contains("/.git/")
                            || path_str.contains("/node_modules/")
                        {
                            return false;
                        }
                        // Apply include/exclude patterns
                        if let Some(ref glob) = include_glob {
                            if !glob.matches(&file_name) {
                                return false;
                            }
                        }
                        if let Some(ref glob) = exclude_glob {
                            if glob.matches(&file_name) {
                                return false;
                            }
                        }
                        true
                    })
                    .map(|e| e.path().to_path_buf())
                    .collect()
            };

            // Search files
            for file_path in files {
                if matches.len() >= max_matches {
                    break;
                }

                // Wrap blocking file I/O in block_in_place to avoid blocking the async runtime
                let content =
                    match tokio::task::block_in_place(|| std::fs::read_to_string(&file_path)) {
                        Ok(c) => c,
                        Err(_) => continue, // Skip binary/unreadable files
                    };

                let lines: Vec<&str> = content.lines().collect();

                for (line_num, line) in lines.iter().enumerate() {
                    if matches.len() >= max_matches {
                        break;
                    }

                    if let Some(m) = regex.find(line) {
                        total_matches += 1;

                        // Skip matches before the offset
                        if total_matches <= skip_offset {
                            continue;
                        }

                        // Get context
                        let start = line_num.saturating_sub(context_lines);
                        let end = (line_num + context_lines + 1).min(lines.len());

                        let context_before: Vec<String> = lines[start..line_num]
                            .iter()
                            .map(|s| s.to_string())
                            .collect();

                        let context_after: Vec<String> = if line_num + 1 < lines.len() {
                            lines[(line_num + 1)..end]
                                .iter()
                                .map(|s| s.to_string())
                                .collect()
                        } else {
                            vec![]
                        };

                        matches.push(GrepMatch {
                            file: file_path.to_string_lossy().to_string(),
                            line: (line_num + 1) as u32,
                            column: (m.start() + 1) as u32,
                            content: line.to_string(),
                            context_before,
                            context_after,
                        });
                    }
                }
            }

            let truncated = matches.len() >= max_matches;
            let has_more = truncated || (total_matches > skip_offset + matches.len());

            Ok(serde_json::json!({
                "matches": matches,
                "count": matches.len(),
                "total_matches": total_matches,
                "truncated": truncated,
                "pagination": {
                    "offset": skip_offset,
                    "limit": max_matches,
                    "total_matches": total_matches,
                    "has_more": has_more
                }
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

            // Combine base path with pattern
            let full_pattern = if pattern_str.starts_with('/') || pattern_str.starts_with("./") {
                pattern_str.to_string()
            } else {
                format!("{}/{}", base_path, pattern_str)
            };

            let glob_pattern = glob::Pattern::new(&full_pattern).context("Invalid glob pattern")?;

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

                if glob_pattern.matches(&path_str) {
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
mod tests {
    use super::*;

    // =========================================================================
    // cached_regex tests
    // =========================================================================

    #[test]
    fn test_cached_regex_simple_pattern() {
        let re = cached_regex("hello").unwrap();
        assert!(re.is_match("hello world"));
        assert!(!re.is_match("goodbye"));
    }

    #[test]
    fn test_cached_regex_returns_same_result_on_second_call() {
        let re1 = cached_regex("test_cache_[0-9]+").unwrap();
        let re2 = cached_regex("test_cache_[0-9]+").unwrap();
        assert_eq!(re1.as_str(), re2.as_str());
    }

    #[test]
    fn test_cached_regex_rejects_too_long_pattern() {
        let long_pattern = "a".repeat(MAX_PATTERN_LENGTH + 1);
        let result = cached_regex(&long_pattern);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too long"));
    }

    #[test]
    fn test_cached_regex_exactly_max_length_ok() {
        let pattern = "a".repeat(MAX_PATTERN_LENGTH);
        assert!(cached_regex(&pattern).is_ok());
    }

    #[test]
    fn test_cached_regex_invalid_pattern() {
        let result = cached_regex("[invalid(");
        assert!(result.is_err());
    }

    #[test]
    fn test_cached_regex_case_insensitive_via_flag() {
        let re = cached_regex("(?i)hello").unwrap();
        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
    }

    #[test]
    fn test_cached_regex_empty_pattern() {
        let re = cached_regex("").unwrap();
        assert!(re.is_match("anything")); // empty regex matches everything
    }

    #[test]
    fn test_cached_regex_special_chars() {
        let re = cached_regex(r"\bfn\b").unwrap();
        assert!(re.is_match("pub fn main()"));
        assert!(!re.is_match("function_name"));
    }

    // =========================================================================
    // build_symbol_patterns tests
    // =========================================================================

    #[test]
    fn test_build_symbol_patterns_function() {
        let patterns = build_symbol_patterns("function", "test").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "function");
    }

    #[test]
    fn test_build_symbol_patterns_struct() {
        let patterns = build_symbol_patterns("struct", "MyStruct").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "struct");
    }

    #[test]
    fn test_build_symbol_patterns_enum() {
        let patterns = build_symbol_patterns("enum", "State").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "enum");
    }

    #[test]
    fn test_build_symbol_patterns_trait() {
        let patterns = build_symbol_patterns("trait", "Handler").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "trait");
    }

    #[test]
    fn test_build_symbol_patterns_impl() {
        let patterns = build_symbol_patterns("impl", "Config").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "impl");
    }

    #[test]
    fn test_build_symbol_patterns_const() {
        let patterns = build_symbol_patterns("const", "MAX").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "const");
    }

    #[test]
    fn test_build_symbol_patterns_type() {
        let patterns = build_symbol_patterns("type", "Result").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "type");
    }

    #[test]
    fn test_build_symbol_patterns_mod() {
        let patterns = build_symbol_patterns("mod", "tests").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].1, "mod");
    }

    #[test]
    fn test_build_symbol_patterns_all() {
        let patterns = build_symbol_patterns("all", "anything").unwrap();
        assert_eq!(patterns.len(), 8);
        let types: Vec<&str> = patterns.iter().map(|p| p.1).collect();
        assert!(types.contains(&"function"));
        assert!(types.contains(&"struct"));
        assert!(types.contains(&"enum"));
        assert!(types.contains(&"trait"));
        assert!(types.contains(&"impl"));
        assert!(types.contains(&"const"));
        assert!(types.contains(&"type"));
        assert!(types.contains(&"mod"));
    }

    #[test]
    fn test_build_symbol_patterns_unknown_falls_back_to_all() {
        let patterns = build_symbol_patterns("unknown_type", "test").unwrap();
        assert_eq!(patterns.len(), 8);
    }

    // =========================================================================
    // Symbol regex matching tests
    // =========================================================================

    #[test]
    fn test_fn_pattern_matches_pub_fn() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.fn_pattern.captures("pub fn my_function(x: i32)").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "my_function");
    }

    #[test]
    fn test_fn_pattern_matches_async_fn() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.fn_pattern.captures("pub async fn fetch_data()").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "fetch_data");
    }

    #[test]
    fn test_fn_pattern_matches_private_fn() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.fn_pattern.captures("fn helper()").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "helper");
    }

    #[test]
    fn test_struct_pattern_matches() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.struct_pattern.captures("pub struct Config {").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "Config");
    }

    #[test]
    fn test_enum_pattern_matches() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.enum_pattern.captures("pub enum State {").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "State");
    }

    #[test]
    fn test_trait_pattern_matches() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.trait_pattern.captures("pub trait Handler {").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "Handler");
    }

    #[test]
    fn test_impl_pattern_matches_simple() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.impl_pattern.captures("impl Config {").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "Config");
    }

    #[test]
    fn test_const_pattern_matches() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.const_pattern.captures("pub const MAX_SIZE: usize = 100;").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "MAX_SIZE");
    }

    #[test]
    fn test_type_pattern_matches() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.type_pattern.captures("pub type Result<T> = std::result::Result<T, Error>;").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "Result");
    }

    #[test]
    fn test_mod_pattern_matches() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.mod_pattern.captures("pub mod tests {").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "tests");
    }

    #[test]
    fn test_fn_pattern_pub_crate() {
        let sr = &*SYMBOL_REGEXES;
        let caps = sr.fn_pattern.captures("pub(crate) fn internal_fn()").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "internal_fn");
    }

    // =========================================================================
    // Tool name/schema/description tests
    // =========================================================================

    #[test]
    fn test_grep_search_tool_name() {
        let tool = GrepSearch;
        assert_eq!(tool.name(), "grep_search");
    }

    #[test]
    fn test_glob_find_tool_name() {
        let tool = GlobFind;
        assert_eq!(tool.name(), "glob_find");
    }

    #[test]
    fn test_symbol_search_tool_name() {
        let tool = SymbolSearch;
        assert_eq!(tool.name(), "symbol_search");
    }

    #[test]
    fn test_grep_search_description_non_empty() {
        assert!(!GrepSearch.description().is_empty());
    }

    #[test]
    fn test_glob_find_description_non_empty() {
        assert!(!GlobFind.description().is_empty());
    }

    #[test]
    fn test_symbol_search_description_non_empty() {
        assert!(!SymbolSearch.description().is_empty());
    }

    #[test]
    fn test_grep_search_schema_has_pattern_and_path() {
        let schema = GrepSearch.schema();
        assert!(schema["properties"].get("pattern").is_some());
        assert!(schema["properties"].get("path").is_some());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("pattern")));
        assert!(required.contains(&serde_json::json!("path")));
    }

    #[test]
    fn test_glob_find_schema_has_pattern() {
        let schema = GlobFind.schema();
        assert!(schema["properties"].get("pattern").is_some());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("pattern")));
    }

    #[test]
    fn test_symbol_search_schema_has_name() {
        let schema = SymbolSearch.schema();
        assert!(schema["properties"].get("name").is_some());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("name")));
    }

    #[test]
    fn test_grep_search_schema_optional_fields() {
        let schema = GrepSearch.schema();
        assert!(schema["properties"].get("recursive").is_some());
        assert!(schema["properties"].get("case_insensitive").is_some());
        assert!(schema["properties"].get("context_lines").is_some());
        assert!(schema["properties"].get("max_matches").is_some());
        assert!(schema["properties"].get("offset").is_some());
        assert!(schema["properties"].get("include").is_some());
        assert!(schema["properties"].get("exclude").is_some());
    }

    // =========================================================================
    // GrepMatch / GrepSearchResult struct tests
    // =========================================================================

    #[test]
    fn test_grep_match_serialization() {
        let m = GrepMatch {
            file: "test.rs".to_string(),
            line: 10,
            column: 5,
            content: "fn test()".to_string(),
            context_before: vec!["// comment".to_string()],
            context_after: vec!["}".to_string()],
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("test.rs"));
        assert!(json.contains("fn test()"));
    }

    #[test]
    fn test_grep_search_result_serialization() {
        let result = GrepSearchResult {
            matches: vec![],
            total_matches: 0,
            file_count: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("total_matches"));
    }

    // =========================================================================
    // Metadata tests
    // =========================================================================

    #[test]
    fn test_grep_search_metadata_read_only() {
        let meta = GrepSearch.metadata();
        assert!(meta.read_only);
    }

    #[test]
    fn test_glob_find_metadata_read_only() {
        let meta = GlobFind.metadata();
        assert!(meta.read_only);
    }

    #[test]
    fn test_symbol_search_metadata_read_only() {
        let meta = SymbolSearch.metadata();
        assert!(meta.read_only);
    }
}

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
