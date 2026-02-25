use selfware::tool_parser::{parse_tool_calls, ParseMethod};
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_parse_tool_calls_never_panics(s in "\PC*") {
        let _ = parse_tool_calls(&s);
    }

    #[test]
    fn test_xml_format_robustness(
        name in "[a-zA-Z_][a-zA-Z0-9_]*",
        args in "\PC*",
        prefix in "\PC*",
        suffix in "\PC*"
    ) {
        let content = format!("{}<tool><name>{}</name><arguments>{}</arguments></tool>{}", 
            prefix, name, args, suffix);
        let result = parse_tool_calls(&content);
        
        // If it parsed at least one tool, one must match our name
        if !result.tool_calls.is_empty() {
            assert!(result.tool_calls.iter().any(|tc| tc.tool_name == name));
        }
    }

    #[test]
    fn test_json_block_robustness(
        name in "[a-zA-Z_][a-zA-Z0-9_]*",
        prefix in "\PC*",
        suffix in "\PC*"
    ) {
        let json_str = format!(r#"{{"tool": "{}", "arguments": {{}}}}"#, name);
        let content = format!("{}```json
{}
```{}", prefix, json_str, suffix);
        let result = parse_tool_calls(&content);
        
        if !result.tool_calls.is_empty() {
            assert!(result.tool_calls.iter().any(|tc| tc.tool_name == name));
            assert_eq!(result.tool_calls[0].parse_method, ParseMethod::Json);
        }
    }

    #[test]
    fn test_qwen3_alt_format_robustness(
        name in "[a-zA-Z_][a-zA-Z0-9_]*",
        args in "\PC*",
        prefix in "\PC*",
        suffix in "\PC*"
    ) {
        let content = format!("{}<tool><name={}</name><arguments>{}</arguments></tool>{}", 
            prefix, name, args, suffix);
        let result = parse_tool_calls(&content);
        
        if !result.tool_calls.is_empty() {
            assert!(result.tool_calls.iter().any(|tc| tc.tool_name == name));
        }
    }

    #[test]
    fn test_bare_function_robustness(
        name in "[a-zA-Z_][a-zA-Z0-9_]*",
        key in "[a-zA-Z_][a-zA-Z0-9_]*",
        val in "[^<]*",
        prefix in "\PC*",
        suffix in "\PC*"
    ) {
        let content = format!("{}<function={}><parameter={}>{}</parameter></function>{}", 
            prefix, name, key, val, suffix);
        let result = parse_tool_calls(&content);
        
        if !result.tool_calls.is_empty() {
            assert!(result.tool_calls.iter().any(|tc| tc.tool_name == name));
        }
    }
}
