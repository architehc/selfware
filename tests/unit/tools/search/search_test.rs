use super::*;
use serde_json::json;

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

    let tool = GlobFind;

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
    let meta = SymbolSearch.metadata();
    assert!(meta.read_only);
}
