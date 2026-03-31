//! Grep search tool - searches file contents using regex patterns.

use crate::tools::Tool;
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

pub mod prompt;

/// Maximum number of compiled regex patterns to cache.
const REGEX_CACHE_MAX: usize = 64;

/// Maximum length of a user-supplied regex pattern (bytes).
const MAX_PATTERN_LENGTH: usize = 1_000;

/// Maximum compiled regex size (bytes).
const MAX_REGEX_SIZE: usize = 1 << 20; // 1 MB

/// Global cache of compiled regex patterns.
static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Return a cached `Regex` for `pattern`, compiling and caching it on first use.
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

    if cache.len() >= REGEX_CACHE_MAX {
        cache.clear();
    }

    cache.insert(pattern.to_owned(), re.clone());
    Ok(re)
}

/// Searches file contents for regex patterns, returning matching lines with context.
pub struct GrepSearch;

/// A single match result from grep search.
#[derive(Debug, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Result of a grep search operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct GrepSearchResult {
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
    pub file_count: usize,
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

            // Build regex
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
                let content =
                    match tokio::task::block_in_place(|| std::fs::read_to_string(&file_path)) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };

                let lines: Vec<&str> = content.lines().collect();

                for (line_num, line) in lines.iter().enumerate() {
                    if let Some(m) = regex.find(line) {
                        total_matches += 1;

                        // Skip if before offset
                        if total_matches <= skip_offset {
                            continue;
                        }

                        // Add to results if we haven't reached max_matches yet
                        if matches.len() < max_matches {
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
}

/// Standalone function for grep search (for use in tests and other modules).
pub fn grep_search(
    pattern: &str,
    path: &str,
    recursive: bool,
    max_matches: usize,
    offset: usize,
) -> GrepSearchResult {
    let mut matches = Vec::new();
    let mut file_count = 0;
    let re = cached_regex(pattern)
        .unwrap_or_else(|_| regex::Regex::new(pattern).expect("Invalid regex pattern"));

    if Path::new(path).is_file() {
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
    matches = matches.into_iter().skip(offset).take(max_matches).collect();

    GrepSearchResult {
        matches,
        total_matches,
        file_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_grep_search_name() {
        let tool = GrepSearch;
        assert_eq!(tool.name(), "grep_search");
    }

    #[test]
    fn test_grep_search_description() {
        let tool = GrepSearch;
        assert!(tool.description().contains("regex"));
    }

    #[test]
    fn test_grep_search_schema() {
        let tool = GrepSearch;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("pattern")));
    }

    #[tokio::test]
    async fn test_grep_search_basic() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line one\nline two\nline three").unwrap();

        let tool = GrepSearch;
        let args = serde_json::json!({
            "pattern": "line two",
            "path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["content"].as_str().unwrap().contains("line two"));
    }

    #[tokio::test]
    async fn test_grep_search_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "HELLO world").unwrap();

        let tool = GrepSearch;
        let args = serde_json::json!({
            "pattern": "hello",
            "path": temp_dir.path().to_str().unwrap(),
            "case_insensitive": true
        });

        let result = tool.execute(args).await.unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[tokio::test]
    async fn test_grep_search_with_context() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3\nline 4\nline 5").unwrap();

        let tool = GrepSearch;
        let args = serde_json::json!({
            "pattern": "line 3",
            "path": temp_dir.path().to_str().unwrap(),
            "context_lines": 1
        });

        let result = tool.execute(args).await.unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);

        let match_obj = &matches[0];
        let context_before = match_obj["context_before"].as_array().unwrap();
        let context_after = match_obj["context_after"].as_array().unwrap();

        assert_eq!(context_before.len(), 1);
        assert_eq!(context_after.len(), 1);
    }

    #[tokio::test]
    async fn test_grep_search_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "match\nmatch\nmatch\nmatch\nmatch").unwrap();

        let tool = GrepSearch;
        let args = serde_json::json!({
            "pattern": "match",
            "path": temp_dir.path().to_str().unwrap(),
            "max_matches": 2,
            "offset": 0
        });

        let result = tool.execute(args).await.unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(result["total_matches"], 5);
    }

    #[test]
    fn test_standalone_grep_search() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world\nfoo bar\nhello again").unwrap();

        let result = grep_search("hello", temp_dir.path().to_str().unwrap(), true, 100, 0);
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.total_matches, 2);
    }
}
