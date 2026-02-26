use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tracing::instrument;
use walkdir::WalkDir;

/// Maximum number of compiled regex patterns to cache.
const REGEX_CACHE_MAX: usize = 64;

/// Global cache of compiled regex patterns, keyed by their source string.
/// When the cache exceeds [`REGEX_CACHE_MAX`] entries, it is cleared entirely
/// (simple but bounded eviction strategy).
static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Return a cached `Regex` for `pattern`, compiling and caching it on first use.
fn cached_regex(pattern: &str) -> Result<Regex> {
    let mut cache = REGEX_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(re) = cache.get(pattern) {
        return Ok(re.clone());
    }

    let re = Regex::new(pattern).context("Invalid regex pattern")?;

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
struct GrepMatch {
    file: String,
    line: u32,
    column: u32,
    content: String,
    context_before: Vec<String>,
    context_after: Vec<String>,
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

                let content = match std::fs::read_to_string(&file_path) {
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

            // We're truncated if we stopped early (matches reached max_matches while there might be more)
            let truncated = matches.len() >= max_matches;

            Ok(serde_json::json!({
                "matches": matches,
                "count": matches.len(),
                "total_matches": total_matches,
                "truncated": truncated
            }))
        })
        .await??;
        Ok(result)
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

                let content = match std::fs::read_to_string(path) {
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
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_grep_search_basic() {
        let tool = GrepSearch;
        assert_eq!(tool.name(), "grep_search");
        assert!(tool.description().contains("Search"));
    }

    #[tokio::test]
    async fn test_grep_search_schema() {
        let tool = GrepSearch;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&"pattern".into()));
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&"path".into()));
    }

    #[tokio::test]
    async fn test_grep_search_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "fn hello() {\n    println!(\"Hello\");\n}\n").unwrap();

        let tool = GrepSearch;
        let result = tool
            .execute(serde_json::json!({
                "pattern": "hello",
                "path": file_path.to_str().unwrap()
            }))
            .await
            .unwrap();

        assert!(result["count"].as_u64().unwrap() >= 1);
        let matches = result["matches"].as_array().unwrap();
        assert!(!matches.is_empty());
        assert_eq!(matches[0]["line"], 1);
    }

    #[tokio::test]
    async fn test_grep_search_case_insensitive() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "fn HELLO() {}\nfn Hello() {}\nfn hello() {}\n").unwrap();

        let tool = GrepSearch;
        let result = tool
            .execute(serde_json::json!({
                "pattern": "hello",
                "path": file_path.to_str().unwrap(),
                "case_insensitive": true
            }))
            .await
            .unwrap();

        assert_eq!(result["count"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_grep_search_with_context() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "line1\nline2\nMATCH\nline4\nline5\n").unwrap();

        let tool = GrepSearch;
        let result = tool
            .execute(serde_json::json!({
                "pattern": "MATCH",
                "path": file_path.to_str().unwrap(),
                "context_lines": 2
            }))
            .await
            .unwrap();

        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m["context_before"].as_array().unwrap().len(), 2);
        assert_eq!(m["context_after"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_grep_search_max_matches() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        let content = (0..50)
            .map(|i| format!("test line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&file_path, content).unwrap();

        let tool = GrepSearch;
        let result = tool
            .execute(serde_json::json!({
                "pattern": "test",
                "path": file_path.to_str().unwrap(),
                "max_matches": 5
            }))
            .await
            .unwrap();

        assert_eq!(result["count"].as_u64().unwrap(), 5);
        assert!(result["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_glob_find_basic() {
        let tool = GlobFind;
        assert_eq!(tool.name(), "glob_find");
        assert!(tool.description().contains("Find files"));
    }

    #[tokio::test]
    async fn test_glob_find_schema() {
        let tool = GlobFind;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&"pattern".into()));
    }

    #[tokio::test]
    async fn test_glob_find_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test1.rs"), "").unwrap();
        fs::write(dir.path().join("test2.rs"), "").unwrap();
        fs::write(dir.path().join("test.txt"), "").unwrap();

        let tool = GlobFind;
        let result = tool
            .execute(serde_json::json!({
                "pattern": "**/*.rs",
                "path": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();

        assert_eq!(result["count"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_symbol_search_basic() {
        let tool = SymbolSearch;
        assert_eq!(tool.name(), "symbol_search");
        assert!(tool.description().contains("function"));
    }

    #[tokio::test]
    async fn test_symbol_search_schema() {
        let tool = SymbolSearch;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&"name".into()));
    }

    #[tokio::test]
    async fn test_symbol_search_function() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "pub fn my_function() {}\nfn another_fn() {}").unwrap();

        let tool = SymbolSearch;
        let result = tool
            .execute(serde_json::json!({
                "name": "function",
                "path": dir.path().to_str().unwrap(),
                "symbol_type": "function"
            }))
            .await
            .unwrap();

        let symbols = result["symbols"].as_array().unwrap();
        assert!(!symbols.is_empty());
        assert!(symbols
            .iter()
            .any(|s| s["name"].as_str().unwrap() == "my_function"));
    }

    #[tokio::test]
    async fn test_symbol_search_struct() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "pub struct MyStruct {\n    field: i32,\n}\n").unwrap();

        let tool = SymbolSearch;
        let result = tool
            .execute(serde_json::json!({
                "name": "Struct",
                "path": dir.path().to_str().unwrap(),
                "symbol_type": "struct"
            }))
            .await
            .unwrap();

        let symbols = result["symbols"].as_array().unwrap();
        assert!(symbols
            .iter()
            .any(|s| s["name"].as_str().unwrap() == "MyStruct"));
    }

    #[tokio::test]
    async fn test_symbol_search_all_types() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(
            &file_path,
            r#"
            pub struct TestStruct {}
            pub enum TestEnum {}
            pub trait TestTrait {}
            pub fn test_function() {}
            impl TestStruct {}
        "#,
        )
        .unwrap();

        let tool = SymbolSearch;
        let result = tool
            .execute(serde_json::json!({
                "name": "Test",
                "path": dir.path().to_str().unwrap(),
                "symbol_type": "all"
            }))
            .await
            .unwrap();

        let symbols = result["symbols"].as_array().unwrap();
        // Should find struct, enum, trait, function
        assert!(symbols.len() >= 4);
    }
}
