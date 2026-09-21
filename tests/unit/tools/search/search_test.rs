use super::*;
use serde_json::json;

/// The search tools now enforce the workspace path policy (they walk and read
/// the filesystem on a user-supplied path). Tests that use temp-dir fixtures
/// outside the workspace allow those fixtures explicitly, mirroring the file
/// tool tests.
fn permissive_safety_config() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

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
    let caps = sr
        .fn_pattern
        .captures("pub fn my_function(x: i32)")
        .unwrap();
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
    let caps = sr
        .const_pattern
        .captures("pub const MAX_SIZE: usize = 100;")
        .unwrap();
    assert_eq!(caps.get(1).unwrap().as_str(), "MAX_SIZE");
}

#[test]
fn test_type_pattern_matches() {
    let sr = &*SYMBOL_REGEXES;
    let caps = sr
        .type_pattern
        .captures("pub type Result<T> = std::result::Result<T, Error>;")
        .unwrap();
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
    let caps = sr
        .fn_pattern
        .captures("pub(crate) fn internal_fn()")
        .unwrap();
    assert_eq!(caps.get(1).unwrap().as_str(), "internal_fn");
}

// =========================================================================
// Tool name/schema/description tests
// =========================================================================

#[test]
fn test_grep_search_tool_name() {
    let tool = GrepSearch::new();
    assert_eq!(tool.name(), "grep_search");
}

#[test]
fn test_glob_find_tool_name() {
    let tool = GlobFind::new();
    assert_eq!(tool.name(), "glob_find");
}

#[test]
fn test_symbol_search_tool_name() {
    let tool = SymbolSearch::new();
    assert_eq!(tool.name(), "symbol_search");
}

#[test]
fn test_grep_search_description_non_empty() {
    assert!(!GrepSearch::new().description().is_empty());
}

#[test]
fn test_glob_find_description_non_empty() {
    assert!(!GlobFind::new().description().is_empty());
}

#[test]
fn test_symbol_search_description_non_empty() {
    assert!(!SymbolSearch::new().description().is_empty());
}

#[test]
fn test_grep_search_schema_has_pattern_and_path() {
    let schema = GrepSearch::new().schema();
    assert!(schema["properties"].get("pattern").is_some());
    assert!(schema["properties"].get("path").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("pattern")));
    assert!(required.contains(&serde_json::json!("path")));
}

#[test]
fn test_glob_find_schema_has_pattern() {
    let schema = GlobFind::new().schema();
    assert!(schema["properties"].get("pattern").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("pattern")));
}

#[test]
fn test_symbol_search_schema_has_name() {
    let schema = SymbolSearch::new().schema();
    assert!(schema["properties"].get("name").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));
}

#[test]
fn test_grep_search_schema_optional_fields() {
    let schema = GrepSearch::new().schema();
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
    let meta = GrepSearch::new().metadata();
    assert!(meta.read_only);
}

#[test]
fn test_glob_find_metadata_read_only() {
    let meta = GlobFind::new().metadata();
    assert!(meta.read_only);
}

#[tokio::test]
async fn test_glob_find_starstar_recursion() {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().join("top.rs");
    let nested = dir.path().join("src").join("nested.rs");
    let deep = dir.path().join("src").join("deep").join("bottom.rs");

    std::fs::create_dir_all(top.parent().unwrap()).unwrap();
    std::fs::write(&top, "").unwrap();
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    std::fs::write(&nested, "").unwrap();
    std::fs::create_dir_all(deep.parent().unwrap()).unwrap();
    std::fs::write(&deep, "").unwrap();

    let tool = GlobFind::with_safety_config(permissive_safety_config());

    // `**/*.rs` must recurse arbitrarily deep.
    let result = tool
        .execute(json!({"pattern": "**/*.rs", "path": dir.path()}))
        .await
        .unwrap();
    let files = result["files"].as_array().unwrap();
    assert_eq!(files.len(), 3, "expected all three .rs files");
    let names: Vec<&str> = files.iter().map(|v| v["path"].as_str().unwrap()).collect();
    assert!(names.iter().any(|p| p.ends_with("top.rs")));
    assert!(names.iter().any(|p| p.ends_with("nested.rs")));
    assert!(names.iter().any(|p| p.ends_with("bottom.rs")));

    // `*.rs` must match only the top-level file.
    let result = tool
        .execute(json!({"pattern": "*.rs", "path": dir.path()}))
        .await
        .unwrap();
    let files = result["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "expected only top-level .rs file");
    assert!(files[0]["path"].as_str().unwrap().ends_with("top.rs"));
}

#[test]
fn test_symbol_search_metadata_read_only() {
    let meta = SymbolSearch::new().metadata();
    assert!(meta.read_only);
}

/// glob_find must reject an out-of-workspace search root with the path-policy
/// error (regression for the search-tools bypass of path validation).
#[tokio::test]
async fn test_glob_find_rejects_out_of_workspace_path() {
    let tool = GlobFind::with_safety_config(SafetyConfig::default());
    let result = tool
        .execute(json!({"pattern": "*.conf", "path": "/etc"}))
        .await;
    let err = result.expect_err("glob_find on /etc must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("outside working directory")
            || message.contains("not in allowed list")
            || message.contains("protected system path"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// symbol_search must reject an out-of-workspace search root with the
/// path-policy error (regression for the search-tools bypass of path
/// validation).
#[tokio::test]
async fn test_symbol_search_rejects_out_of_workspace_path() {
    let tool = SymbolSearch::with_safety_config(SafetyConfig::default());
    let result = tool.execute(json!({"name": "root", "path": "/etc"})).await;
    let err = result.expect_err("symbol_search on /etc must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("outside working directory")
            || message.contains("not in allowed list")
            || message.contains("protected system path"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// symbol_search must still work on a legal in-workspace-style root (allowed
/// via the explicit permissive config).
#[tokio::test]
async fn test_symbol_search_works_inside_workspace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("lib.rs"), "pub fn helper_symbol() {}\n").unwrap();

    let tool = SymbolSearch::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(json!({"name": "helper_symbol", "path": dir.path()}))
        .await
        .unwrap();
    let symbols = result["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 1, "expected to find helper_symbol");
    assert!(symbols[0]["name"]
        .as_str()
        .unwrap()
        .contains("helper_symbol"));
}

/// P1 regression: glob_find must not enumerate entries under a denied
/// subdirectory inside an allowed root (same per-file rule `file_read`
/// enforces).
#[tokio::test]
async fn test_glob_find_denied_child_not_enumerated() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("denied_dir")).unwrap();
    std::fs::write(dir.path().join("public.txt"), "").unwrap();
    std::fs::write(dir.path().join("denied_dir/secret.txt"), "").unwrap();

    let tool = GlobFind::with_safety_config(SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        denied_paths: vec!["denied_dir".to_string()],
        ..SafetyConfig::default()
    });
    let result = tool
        .execute(json!({"pattern": "**/*.txt", "path": dir.path()}))
        .await
        .unwrap();
    let files = result["files"].as_array().unwrap();
    assert!(!files.is_empty(), "allowed files must still be enumerated");
    for f in files {
        let path = f["path"].as_str().unwrap();
        assert!(
            !path.contains("denied_dir"),
            "denied entry leaked from glob_find: {}",
            path
        );
    }
    let public = files
        .iter()
        .any(|f| f["path"].as_str().unwrap().ends_with("public.txt"));
    assert!(public, "the allowed file must still be enumerated");
}

/// P1 regression: symbol_search reads every descendant `.rs` file — a denied
/// subdirectory inside an allowed root must not be read, so no symbol from it
/// is reported.
#[tokio::test]
async fn test_symbol_search_denied_child_excluded() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("denied_dir")).unwrap();
    std::fs::write(dir.path().join("lib.rs"), "pub fn visible_helper() {}\n").unwrap();
    std::fs::write(
        dir.path().join("denied_dir/lib.rs"),
        "pub fn hidden_helper() {}\n",
    )
    .unwrap();

    let tool = SymbolSearch::with_safety_config(SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        denied_paths: vec!["denied_dir".to_string()],
        ..SafetyConfig::default()
    });
    let result = tool
        .execute(json!({"name": "helper", "path": dir.path()}))
        .await
        .unwrap();
    let symbols = result["symbols"].as_array().unwrap();
    assert!(!symbols.is_empty(), "allowed symbols must still be found");
    for s in symbols {
        let file = s["file"].as_str().unwrap();
        assert!(
            !file.contains("denied_dir"),
            "symbol from a denied subdirectory leaked: {}",
            file
        );
    }
    let visible = symbols
        .iter()
        .any(|s| s["name"].as_str().unwrap().contains("visible_helper"));
    assert!(visible, "the allowed symbol must be present");
}
