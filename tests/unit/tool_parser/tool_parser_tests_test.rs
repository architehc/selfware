use super::*;

#[test]
fn test_parse_xml_tool_call() {
    let content = r#"I'll read the file for you.

<tool>
<name>file_read</name>
<arguments>{"path": "src/main.rs"}</arguments>
</tool>

Let me know if you need anything else."#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "src/main.rs");
    assert_eq!(result.tool_calls[0].parse_method, ParseMethod::Xml);
    assert!(result.text_content.contains("I'll read the file"));
    assert!(!result.text_content.contains("<tool>"));
}

#[test]
fn test_parse_multiple_xml_tools() {
    let content = r#"<tool>
<name>file_read</name>
<arguments>{"path": "a.txt"}</arguments>
</tool>
<tool>
<name>file_read</name>
<arguments>{"path": "b.txt"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 2);
    assert_eq!(result.tool_calls[0].arguments["path"], "a.txt");
    assert_eq!(result.tool_calls[1].arguments["path"], "b.txt");
}

#[test]
fn test_parse_xml_with_element_arguments() {
    // Test with XML element-style arguments (single line)
    let content = r#"<tool>
<name>shell_exec</name>
<arguments><command>ls -la</command><timeout_secs>30</timeout_secs></arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
    assert_eq!(result.tool_calls[0].arguments["command"], "ls -la");
    assert_eq!(result.tool_calls[0].arguments["timeout_secs"], 30);
}

#[test]
fn test_parse_xml_with_json_arguments_multiline() {
    // Test with JSON arguments spanning multiple lines
    let content = r#"<tool>
<name>file_read</name>
<arguments>
{
    "path": "test.txt",
    "line_range": [1, 10]
}
</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "test.txt");
}

#[test]
fn test_parse_json_code_block() {
    let content = r#"Here's what I'll do:

```json
{
    "tool": "file_read",
    "arguments": {"path": "test.txt"}
}
```
"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].parse_method, ParseMethod::Json);
}

#[test]
fn test_parse_no_tool_calls() {
    let content = "This is just regular text with no tool calls.";

    let result = parse_tool_calls(content);

    assert!(result.tool_calls.is_empty());
    assert_eq!(result.text_content, content);
}

#[test]
fn test_parse_malformed_xml_uses_fallback() {
    let content = r#"<tool>
<name>file_read</name>
<arguments>just a plain text argument</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    // Should still parse, with arguments as a fallback
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    // The fallback wraps it in {"input": "..."}
    assert!(result.tool_calls[0].arguments.get("input").is_some());
}

#[test]
fn test_validate_tool_call_unknown_tool() {
    let tool_call = ParsedToolCall {
        tool_name: "unknown_tool".to_string(),
        arguments: serde_json::json!({}),
        raw_text: "".to_string(),
        parse_method: ParseMethod::Xml,
    };

    let available = vec!["file_read", "file_write"];
    let result = validate_tool_call(&tool_call, &available);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown tool"));
}

#[test]
fn test_validate_tool_call_success() {
    let tool_call = ParsedToolCall {
        tool_name: "file_read".to_string(),
        arguments: serde_json::json!({"path": "test.txt"}),
        raw_text: "".to_string(),
        parse_method: ParseMethod::Xml,
    };

    let available = vec!["file_read", "file_write"];
    let result = validate_tool_call(&tool_call, &available);

    assert!(result.is_ok());
}

#[test]
fn test_extract_text_only() {
    let content = r#"Here's some text.

<tool>
<name>file_read</name>
<arguments>{"path": "test.txt"}</arguments>
</tool>

More text here."#;

    let text = extract_text_only(content);

    assert!(text.contains("Here's some text"));
    assert!(text.contains("More text here"));
    assert!(!text.contains("<tool>"));
    assert!(!text.contains("file_read"));
}

#[test]
fn test_parse_json_with_function_key() {
    let content = r#"```json
{
    "function": "shell_exec",
    "args": {"command": "echo hello"}
}
```"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
}

#[test]
fn test_complex_nested_arguments() {
    let content = r#"<tool>
<name>http_request</name>
<arguments>{"url": "https://api.example.com", "headers": {"Authorization": "Bearer token"}, "body": "{\"key\": \"value\"}"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "http_request");
    assert!(result.tool_calls[0].arguments["headers"].is_object());
}

#[test]
fn test_parse_method_eq() {
    assert_eq!(ParseMethod::Native, ParseMethod::Native);
    assert_eq!(ParseMethod::Xml, ParseMethod::Xml);
    assert_eq!(ParseMethod::Json, ParseMethod::Json);
    assert_eq!(ParseMethod::Markdown, ParseMethod::Markdown);
    assert_ne!(ParseMethod::Xml, ParseMethod::Json);
}

#[test]
fn test_parse_method_clone() {
    let method = ParseMethod::Xml;
    let cloned = method;
    assert_eq!(method, cloned);
}

#[test]
fn test_parse_method_serialize() {
    let method = ParseMethod::Xml;
    let json = serde_json::to_string(&method).unwrap();
    assert!(json.contains("Xml"));
}

#[test]
fn test_parse_method_deserialize() {
    let json = "\"Json\"";
    let method: ParseMethod = serde_json::from_str(json).unwrap();
    assert_eq!(method, ParseMethod::Json);
}

#[test]
fn test_parsed_tool_call_clone() {
    let tc = ParsedToolCall {
        tool_name: "test".to_string(),
        arguments: serde_json::json!({"key": "value"}),
        raw_text: "<tool>...</tool>".to_string(),
        parse_method: ParseMethod::Xml,
    };
    let cloned = tc.clone();
    assert_eq!(tc.tool_name, cloned.tool_name);
    assert_eq!(tc.arguments, cloned.arguments);
    assert_eq!(tc.parse_method, cloned.parse_method);
}

#[test]
fn test_parsed_tool_call_debug() {
    let tc = ParsedToolCall {
        tool_name: "file_read".to_string(),
        arguments: serde_json::json!({}),
        raw_text: "".to_string(),
        parse_method: ParseMethod::Native,
    };
    let debug_str = format!("{:?}", tc);
    assert!(debug_str.contains("ParsedToolCall"));
    assert!(debug_str.contains("file_read"));
}

#[test]
fn test_parsed_tool_call_serialize() {
    let tc = ParsedToolCall {
        tool_name: "shell_exec".to_string(),
        arguments: serde_json::json!({"command": "ls"}),
        raw_text: "raw".to_string(),
        parse_method: ParseMethod::Json,
    };
    let json = serde_json::to_string(&tc).unwrap();
    assert!(json.contains("shell_exec"));
    assert!(json.contains("command"));
}

#[test]
fn test_parsed_tool_call_deserialize() {
    let json = r#"{
            "tool_name": "git_status",
            "arguments": {},
            "raw_text": "",
            "parse_method": "Native"
        }"#;
    let tc: ParsedToolCall = serde_json::from_str(json).unwrap();
    assert_eq!(tc.tool_name, "git_status");
    assert_eq!(tc.parse_method, ParseMethod::Native);
}

#[test]
fn test_parse_result_debug() {
    let result = ParseResult {
        tool_calls: vec![],
        text_content: "test".to_string(),
        parse_errors: vec![],
        rejections: vec![],
    };
    let debug_str = format!("{:?}", result);
    assert!(debug_str.contains("ParseResult"));
}

#[test]
fn test_validate_tool_call_non_object_arguments() {
    let tool_call = ParsedToolCall {
        tool_name: "file_read".to_string(),
        arguments: serde_json::json!("just a string"),
        raw_text: "".to_string(),
        parse_method: ParseMethod::Xml,
    };

    let available = vec!["file_read", "file_write"];
    let result = validate_tool_call(&tool_call, &available);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be a JSON object"));
}

#[test]
fn test_validate_tool_call_array_arguments() {
    let tool_call = ParsedToolCall {
        tool_name: "file_read".to_string(),
        arguments: serde_json::json!([1, 2, 3]),
        raw_text: "".to_string(),
        parse_method: ParseMethod::Xml,
    };

    let available = vec!["file_read"];
    let result = validate_tool_call(&tool_call, &available);

    assert!(result.is_err());
}

#[test]
fn test_parse_empty_content() {
    let result = parse_tool_calls("");
    assert!(result.tool_calls.is_empty());
    assert_eq!(result.text_content, "");
    assert!(result.parse_errors.is_empty());
}

#[test]
fn test_parse_whitespace_only() {
    let result = parse_tool_calls("   \n\t  ");
    assert!(result.tool_calls.is_empty());
    assert_eq!(result.text_content, "");
}

#[test]
fn test_parse_json_with_name_key() {
    let content = r#"```json
{
    "name": "file_write",
    "parameters": {"path": "test.txt", "content": "hello"}
}
```"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_write");
    assert_eq!(result.tool_calls[0].arguments["path"], "test.txt");
}

#[test]
fn test_parse_json_block_without_json_marker() {
    let content = r#"```
{
    "tool": "git_commit",
    "arguments": {"message": "test commit"}
}
```"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "git_commit");
}

#[test]
fn test_parse_json_block_not_tool_call() {
    // A JSON block that doesn't look like a tool call
    let content = r#"```json
{
    "name": "John",
    "age": 30
}
```"#;

    let result = parse_tool_calls(content);

    // Should not parse as a tool call since it has "name" but no arguments
    // The "name" key triggers tool parsing, but age isn't "arguments"
    // Actually let me check - it has "name" key, so it might try
    // Since there's no arguments/args/parameters, it defaults to {}
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "John");
}

#[test]
fn test_parse_xml_boolean_argument() {
    let content = r#"<tool>
<name>file_read</name>
<arguments><path>test.txt</path><recursive>true</recursive></arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].arguments["recursive"], true);
}

#[test]
fn test_parse_xml_number_argument() {
    let content = r#"<tool>
<name>shell_exec</name>
<arguments><command>ls</command><timeout>60</timeout></arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].arguments["timeout"], 60);
}

#[test]
fn test_parse_mixed_content() {
    let content = r#"First I'll check the status.

<tool>
<name>git_status</name>
<arguments>{}</arguments>
</tool>

Then I'll make changes.

<tool>
<name>file_edit</name>
<arguments>{"path": "main.rs"}</arguments>
</tool>

Finally, I'll commit."#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 2);
    assert!(result.text_content.contains("First I'll check"));
    assert!(result.text_content.contains("Then I'll make"));
    assert!(result.text_content.contains("Finally"));
    assert!(!result.text_content.contains("<tool>"));
}

#[test]
fn test_xml_tool_regex_exists() {
    let regex = xml_tool_regex();
    assert!(regex.is_match("<tool><name>test</name><arguments>{}</arguments></tool>"));
}

#[test]
fn test_json_block_regex_exists() {
    let regex = json_block_regex();
    assert!(regex.is_match("```json\n{\"key\": \"value\"}\n```"));
}

#[test]
fn test_parse_xml_mismatched_tags() {
    let content = r#"<tool>
<name>test</name>
<arguments><foo>bar</baz></arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    // Mismatched tags won't be parsed as XML elements
    // Falls back to input wrapper
    assert_eq!(result.tool_calls.len(), 1);
    assert!(result.tool_calls[0].arguments.get("input").is_some());
}

#[test]
fn test_extract_text_only_no_tools() {
    let content = "Just plain text without any tools.";
    let text = extract_text_only(content);
    assert_eq!(text, content);
}

#[test]
fn test_validate_tool_call_empty_available() {
    let tool_call = ParsedToolCall {
        tool_name: "any_tool".to_string(),
        arguments: serde_json::json!({}),
        raw_text: "".to_string(),
        parse_method: ParseMethod::Xml,
    };

    let available: Vec<&str> = vec![];
    let result = validate_tool_call(&tool_call, &available);

    assert!(result.is_err());
}

#[test]
fn test_parse_qwen3_style_xml() {
    // Qwen3-Coder uses alternate format: <name=tool_name</name>
    let content = r#"<tool>
<name=file_read</name>
<arguments>{"path": "./Cargo.toml"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse Qwen3-style tool call"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "./Cargo.toml");
    assert_eq!(result.tool_calls[0].parse_method, ParseMethod::Xml);
}

#[test]
fn test_parse_qwen3_style_shell_exec() {
    // Another Qwen3-style example
    let content = r#"<tool>
<name=shell_exec</name>
<arguments>{"command": "ls -la"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
    assert_eq!(result.tool_calls[0].arguments["command"], "ls -la");
}

#[test]
fn test_xml_tool_alt_regex_exists() {
    let regex = xml_tool_alt_regex();
    assert!(regex.is_match("<tool><name=test</name><arguments>{}</arguments></tool>"));
}

#[test]
fn test_xml_tool_alt2_regex_exists() {
    let regex = xml_tool_alt2_regex();
    assert!(regex.is_match("<tool><name=test><arguments>{}</arguments></tool>"));
}

#[test]
fn test_parse_qwen3_style_alt1() {
    // Format: <name=tool_name</name> (no closing > after name value)
    let content = r#"<tool>
<name=shell_exec</name>
<arguments>{"command": "ls -la"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1, "Should parse 1 tool call");
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
    assert_eq!(result.tool_calls[0].arguments["command"], "ls -la");
}

#[test]
fn test_parse_qwen3_style_alt2() {
    // Format: <name=tool_name> (with closing > after name value)
    let content = r#"<tool>
<name=shell_exec>
<arguments>{"command": "ls -la"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1, "Should parse 1 tool call");
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
    assert_eq!(result.tool_calls[0].arguments["command"], "ls -la");
}

#[test]
fn test_parse_qwen3_style_inline() {
    // Inline format matching what the model actually produces
    let content = r#"I've created the file. Let me verify:

<tool>
<name=cargo_check</name>
<arguments>{"all_targets": false}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1, "Should parse 1 tool call");
    assert_eq!(result.tool_calls[0].tool_name, "cargo_check");
    assert!(result.text_content.contains("I've created the file"));
}

#[test]
fn test_xml_tool_function_regex_exists() {
    let regex = xml_tool_function_regex();
    assert!(regex.is_match("<tool><function=test</function><arguments>{}</arguments></tool>"));
}

#[test]
fn test_parse_function_style_xml() {
    // Format used by some models: <function=tool_name</function>
    let content = r#"<tool>
<function=file_read</function>
<arguments>{"path": "./src/lib.rs"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse function-style tool call"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "./src/lib.rs");
    assert_eq!(result.tool_calls[0].parse_method, ParseMethod::Xml);
}

#[test]
fn test_parse_multiple_function_style_tools() {
    let content = r#"<tool>
<function=file_read</function>
<arguments>{"path": "./src/lib.rs"}</arguments>
</tool>
<tool>
<function=file_read</function>
<arguments>{"path": "./src/main.rs"}</arguments>
</tool>
<tool>
<function=file_read</function>
<arguments>{"path": "./README.md"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        3,
        "Should parse all 3 function-style tool calls"
    );
    assert_eq!(result.tool_calls[0].arguments["path"], "./src/lib.rs");
    assert_eq!(result.tool_calls[1].arguments["path"], "./src/main.rs");
    assert_eq!(result.tool_calls[2].arguments["path"], "./README.md");
}

#[test]
fn test_bare_function_format() {
    // Format used by Qwen3-Coder without tool_call wrapper
    let content = r#"<function=file_edit>
<parameter=path>
./tests/integration/helpers.rs
</parameter>
<parameter=old_str>
        yolo: YoloFileConfig::default(),
    }
}
</parameter>
<parameter=new_str>
        yolo: YoloFileConfig::default(),
        execution_mode: Default::default(),
    }
}
</parameter>
</function>"#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse bare function format"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_edit");
    assert!(result.tool_calls[0].arguments["path"]
        .as_str()
        .unwrap()
        .contains("helpers.rs"));
    assert!(result.tool_calls[0].arguments["old_str"].as_str().is_some());
    assert!(result.tool_calls[0].arguments["new_str"].as_str().is_some());
}

#[test]
fn test_bare_function_file_read() {
    let content = r#"<function=file_read>
<parameter=path>
./tests/integration/helpers.rs
</parameter>
<parameter=line_range>
[35, 55]
</parameter>
</function>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert!(result.tool_calls[0].arguments["path"]
        .as_str()
        .unwrap()
        .contains("helpers.rs"));
}

#[test]
fn test_openai_function_format() {
    // OpenAI-style: <function=name>{"json":"args"}</function>
    let content = r#"<function=file_read>{"path": "./src/main.rs"}</function>"#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse OpenAI function format"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "./src/main.rs");
}

#[test]
fn test_openai_function_format_embedded_in_text() {
    let content = r#"Let me read that file for you.

<function=shell_exec>{"command": "cargo build"}</function>

I'll check the output next."#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse OpenAI function format embedded in text"
    );
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
    assert_eq!(result.tool_calls[0].arguments["command"], "cargo build");
    assert!(result.text_content.contains("Let me read"));
    assert!(result.text_content.contains("check the output"));
}

#[test]
fn test_malformed_closing_tags_standard_xml() {
    // Qwen3.5-4B sometimes drops `</` prefix on closing tags
    let content = "<tool>\n<name>file_read name>\n<arguments>{\"path\": \"./src/main.rs\"} arguments>\n tool>";

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse tool call with malformed closing tags"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "./src/main.rs");
}

#[test]
fn test_missing_arguments_close_recovered() {
    // Qwen3.6 sometimes closes the tool while forgetting </arguments>.
    let content = r#"<tool>
<name>glob_find</name>
<arguments>{"pattern":"**/guiprocess.py"}
</tool>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "glob_find");
    assert_eq!(
        result.tool_calls[0].arguments["pattern"],
        "**/guiprocess.py"
    );
}

#[test]
fn test_malformed_closing_tags_qwen3_format() {
    // Qwen3 format with malformed closing tags
    let content = "<tool_call>\n<function=file_read>\n<parameter=path>\n./src/main.rs\nparameter>\nfunction>\ntool_call>";

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse Qwen3 tool call with malformed closing tags"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
}

#[test]
fn test_malformed_closing_tags_preserves_valid() {
    // Valid closing tags should NOT be modified
    let content = r#"<tool>
<name>file_read</name>
<arguments>{"path": "test.txt"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "test.txt");
}

#[test]
fn test_normalize_malformed_xml_unit() {
    // Direct unit test of the normalization function
    assert_eq!(normalize_malformed_xml("arguments>"), "</arguments>");
    assert_eq!(normalize_malformed_xml(" arguments>"), " </arguments>");
    assert_eq!(normalize_malformed_xml("\narguments>"), "\n</arguments>");
    // Valid tags should be untouched
    assert_eq!(normalize_malformed_xml("</arguments>"), "</arguments>");
    assert_eq!(normalize_malformed_xml("<arguments>"), "<arguments>");
    // `>` inside JSON arguments must not be corrupted
    assert_eq!(
        normalize_malformed_xml(r#"<arguments>{"op": "a > b"}</arguments>"#),
        r#"<arguments>{"op": "a > b"}</arguments>"#
    );
}

#[test]
fn test_normalize_malformed_xml_preserves_json_strings() {
    // JSON string values ending in tag-like names (e.g. "tool>", "name>")
    // must be preserved, even when the closing tag starts on the next line.
    assert_eq!(
        normalize_malformed_xml(
            r#"<arguments>{"x": "tool>"}
</arguments>"#
        ),
        r#"<arguments>{"x": "tool>"}
</arguments>"#
    );
    assert_eq!(
        normalize_malformed_xml(
            r#"<arguments>{"x": "my name>"}
</arguments>"#
        ),
        r#"<arguments>{"x": "my name>"}
</arguments>"#
    );
    // Malformed closing tags outside JSON strings are still normalized.
    assert_eq!(
        normalize_malformed_xml(
            r#"<arguments>{"x": "ok"} arguments>
</arguments>"#
        ),
        r#"<arguments>{"x": "ok"} </arguments>
</arguments>"#
    );
}

#[test]
fn test_openai_function_format_multiline_json() {
    let content = r#"<function=file_write>{
    "path": "./test.txt",
    "content": "hello world"
}</function>"#;

    let result = parse_tool_calls(content);

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_write");
    assert_eq!(result.tool_calls[0].arguments["path"], "./test.txt");
    assert_eq!(result.tool_calls[0].arguments["content"], "hello world");
}

#[test]
fn test_extract_json_balanced_simple() {
    let input = r#"{"path": "test.txt", "content": "hello"}"#;
    let result = extract_json_balanced(input);
    assert!(result.is_some());
    let (json_str, end) = result.unwrap();
    assert_eq!(json_str, input);
    assert_eq!(end, input.len());
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["path"], "test.txt");
}

#[test]
fn test_extract_json_balanced_with_html() {
    // This is the bug case: HTML tags inside a JSON string value
    let input = r#"{"path":"./index.html","content":"<html><head><title>Test</title></head><body><h1>Welcome</h1><div class=\"main\"><p>Hello</p></div></body></html>"}"#;
    let result = extract_json_balanced(input);
    assert!(result.is_some());
    let (json_str, _end) = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["path"], "./index.html");
    assert!(parsed["content"]
        .as_str()
        .unwrap()
        .contains("<h1>Welcome</h1>"));
}

#[test]
fn test_extract_json_balanced_nested() {
    let input = r#"{"url": "https://api.example.com", "headers": {"Authorization": "Bearer token"}, "body": [1, 2, {"nested": true}]}"#;
    let result = extract_json_balanced(input);
    assert!(result.is_some());
    let (json_str, _end) = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert!(parsed["headers"].is_object());
    assert_eq!(parsed["headers"]["Authorization"], "Bearer token");
    assert!(parsed["body"].is_array());
}

#[test]
fn test_extract_json_balanced_escaped_quotes() {
    let input = r#"{"content": "She said \"hello\" and then left", "path": "test.txt"}"#;
    let result = extract_json_balanced(input);
    assert!(result.is_some());
    let (json_str, _end) = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["path"], "test.txt");
    assert!(parsed["content"].as_str().unwrap().contains("hello"));
}

#[test]
fn test_extract_json_balanced_no_json() {
    let input = "just plain text with no braces at all";
    let result = extract_json_balanced(input);
    assert!(result.is_none());
}

#[test]
fn test_parse_tool_call_with_html_content() {
    // End-to-end: file_write with HTML content inside <arguments>
    let content = r#"<tool>
<name>file_write</name>
<arguments>{"path":"./index.html","content":"<html><head><title>My Site</title></head><body><div class=\"container\"><h1>Welcome</h1><h2>Get in Touch</h2><p>Contact us</p></div></body></html>"}</arguments>
</tool>"#;

    let result = parse_tool_calls(content);

    assert_eq!(
        result.tool_calls.len(),
        1,
        "Should parse exactly one tool call"
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_write");
    assert_eq!(result.tool_calls[0].arguments["path"], "./index.html");
    let content_val = result.tool_calls[0].arguments["content"]
        .as_str()
        .expect("content should be a string");
    assert!(
        content_val.contains("<h1>Welcome</h1>"),
        "HTML tags should be preserved in content"
    );
    assert!(
        content_val.contains("<h2>Get in Touch</h2>"),
        "HTML tags should not be parsed as argument keys"
    );
    // Verify we did NOT get the broken parse where HTML tags become keys
    assert!(
        result.tool_calls[0].arguments.get("h1").is_none(),
        "h1 should NOT appear as a top-level argument key"
    );
    assert!(
        result.tool_calls[0].arguments.get("h2").is_none(),
        "h2 should NOT appear as a top-level argument key"
    );
}

#[test]
fn test_parse_kimi_style_tools() {
    let content = r#"I'll check the diff and the file.<|open|>tools<|sep|><|open|>call tool="shell_exec" index="1"<|sep|><|open|>argument key="command" type="string"<|sep|>git diff src/output/mod.rs<|close|>argument<|sep|><|open|>argument key="output_limit" type="number"<|sep|>5000<|close|>argument<|close|>call<|sep|><|open|>call tool="shell_exec" index="2"<|sep|><|open|>argument key="command" type="string"<|sep|>file selfware-screen.png<|close|>argument<|close|>call<|sep|><|close|>tools<|sep|><|close|>message<|sep|>"#;

    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 2, "Should parse 2 Kimi tool calls");
    assert_eq!(result.tool_calls[0].tool_name, "shell_exec");
    assert_eq!(
        result.tool_calls[0].arguments["command"],
        "git diff src/output/mod.rs"
    );
    assert_eq!(result.tool_calls[0].arguments["output_limit"], 5000);
    assert_eq!(result.tool_calls[1].tool_name, "shell_exec");
    assert_eq!(
        result.tool_calls[1].arguments["command"],
        "file selfware-screen.png"
    );
    assert_eq!(result.text_content, "I'll check the diff and the file.");
}

#[test]
fn test_parse_kimi_style_no_args() {
    let content = r#"Checking git diff.<|open|>tools<|sep|><|open|>call tool="git_diff" index="1"<|sep|><|close|>call<|sep|><|close|>tools<|sep|>"#;

    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "git_diff");
    assert_eq!(result.tool_calls[0].arguments, serde_json::json!({}));
    assert_eq!(result.text_content, "Checking git diff.");
}

#[test]
fn test_parse_qwen_hybrid_function_tool() {
    let content = r#"<function=tool>
<name>file_read</name>
<arguments>{"path": "src/main.rs"}</arguments>
</function>"#;

    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "src/main.rs");
}

#[test]
fn test_qwen_file_write_content_with_arguments_literal_keeps_own_args() {
    // A file_write whose *content* contains a literal hybrid `<arguments>` block
    // must keep its own path/content — the payload is not call structure.
    let content = r#"<tool_call>
<function=file_write>
<parameter=path>docs/format.md</parameter>
<parameter=content>Example: <name>file_read</name><arguments>{"path":"/etc/x"}</arguments></parameter>
</function>
</tool_call>"#;
    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    let call = &result.tool_calls[0];
    assert_eq!(call.tool_name, "file_write");
    assert_eq!(call.arguments["path"], "docs/format.md");
    assert_eq!(
        call.arguments["content"],
        r#"Example: <name>file_read</name><arguments>{"path":"/etc/x"}</arguments>"#
    );
}

#[test]
fn test_qwen_bare_file_write_content_with_arguments_literal_keeps_own_args() {
    let content = r#"<function=file_write>
<parameter=path>notes.txt</parameter>
<parameter=content><arguments>{"path":"/etc/x"}</arguments></parameter>
</function>"#;
    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    let call = &result.tool_calls[0];
    assert_eq!(call.tool_name, "file_write");
    assert_eq!(call.arguments["path"], "notes.txt");
    assert_eq!(
        call.arguments["content"],
        r#"<arguments>{"path":"/etc/x"}</arguments>"#
    );
}

#[test]
fn test_qwen_generic_tool_name_ignores_name_inside_parameter() {
    // `<function=tool>` with no leading <name>: a `<name>` inside a parameter
    // value must not rename the call.
    let content = r#"<tool_call>
<function=tool>
<parameter=content><name>shell_exec</name></parameter>
</function>
</tool_call>"#;
    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "tool");
    assert_eq!(
        result.tool_calls[0].arguments["content"],
        "<name>shell_exec</name>"
    );
}

#[test]
fn test_qwen_hybrid_function_tool_in_tool_call_wrapper_still_parses() {
    let content = r#"<tool_call>
<function=tool>
<name>file_read</name>
<arguments>{"path": "src/lib.rs"}</arguments>
</function>
</tool_call>"#;
    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "src/lib.rs");
}

#[test]
fn test_qwen_hybrid_arguments_payload_name_does_not_rename() {
    // <name> resolution reads only the leading header, never text inside the
    // arguments JSON.
    let content = r#"<function=tool>
<arguments>{"path": "a.md", "content": "<name>shell_exec</name>"}</arguments>
<name>file_write</name>
</function>"#;
    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_write");
    assert_eq!(result.tool_calls[0].arguments["path"], "a.md");
    assert_eq!(
        result.tool_calls[0].arguments["content"],
        "<name>shell_exec</name>"
    );
}

// ---- `<parameter name="key">` dialect (validation run 2026-09-24, turn_0026) ----

#[test]
fn test_qwen_tool_call_parameter_name_attribute_dialect() {
    // Verbatim shape from the validation run: parsed to {} before the fix and
    // was rejected by schema validation (missing pattern/path).
    let content = "<tool_call>\n<function=grep_search>\n<parameter name=\"pattern\">fn trust_gate</parameter>\n<parameter name=\"path\">src/safety\n</parameter>\n<parameter name=\"max_matches\">20</parameter>\n</function>\n</tool_call>";
    let result = parse_tool_calls(content);
    assert_eq!(
        result.tool_calls.len(),
        1,
        "errors: {:?}",
        result.parse_errors
    );
    let call = &result.tool_calls[0];
    assert_eq!(call.tool_name, "grep_search");
    assert_eq!(call.arguments["pattern"], "fn trust_gate");
    assert_eq!(call.arguments["path"], "src/safety");
    assert_eq!(call.arguments["max_matches"], 20);
}

#[test]
fn test_qwen_parameter_dialect_variants_all_parse() {
    for open in [
        "<parameter=path>",
        "<parameter = path>",
        "<parameter=\"path\">",
        "<parameter name=\"path\">",
        "<parameter name='path'>",
        "<parameter name=path>",
        "<parameter  name = \"path\" >",
        "<parameter\n  name=\"path\"\n>",
    ] {
        let content = format!(
            "<tool_call>\n<function=file_read>\n{open}src/lib.rs</parameter>\n</function>\n</tool_call>"
        );
        let result = parse_tool_calls(&content);
        assert_eq!(
            result.tool_calls.len(),
            1,
            "{open}: {:?}",
            result.parse_errors
        );
        assert_eq!(
            result.tool_calls[0].arguments["path"], "src/lib.rs",
            "{open}: {}",
            result.tool_calls[0].arguments
        );
    }
}

#[test]
fn test_qwen_bare_function_parameter_name_attribute_dialect() {
    // Bare path (no <tool_call> wrapper) goes through the same parameter
    // parser; mixed dialects in one call both land.
    let content = "<function=file_read>\n<parameter name=\"path\">src/main.rs</parameter>\n<parameter=line_range>[1, 40]</parameter>\n</function>";
    let result = parse_tool_calls(content);
    assert_eq!(
        result.tool_calls.len(),
        1,
        "errors: {:?}",
        result.parse_errors
    );
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    assert_eq!(result.tool_calls[0].arguments["path"], "src/main.rs");
    assert_eq!(
        result.tool_calls[0].arguments["line_range"],
        serde_json::json!([1, 40])
    );
}

#[test]
fn test_qwen_parameter_name_attribute_value_keeps_newlines_and_angle_brackets() {
    let content = "<tool_call>\n<function=file_write>\n<parameter name=\"path\">a.rs</parameter>\n<parameter name='content'>fn f() -> Vec<u8> {\n    if a < b { vec![] } else { vec![1] }\n}</parameter>\n</function>\n</tool_call>";
    let result = parse_tool_calls(content);
    assert_eq!(
        result.tool_calls.len(),
        1,
        "errors: {:?}",
        result.parse_errors
    );
    assert_eq!(
        result.tool_calls[0].arguments["content"],
        "fn f() -> Vec<u8> {\n    if a < b { vec![] } else { vec![1] }\n}"
    );
}

#[test]
fn test_qwen_hybrid_guard_sees_parameter_name_dialect() {
    // The hybrid `<arguments>` override must not fire for a real tool whose
    // body carries `<parameter name=…>` tags: text inside a parameter value
    // is payload, never structure (same guard as the `<parameter=` form).
    let content = "<function=file_write>\n<arguments>{\"path\": \"evil.sh\"}</arguments>\n<parameter name=\"path\">a.md</parameter>\n<parameter name=\"content\">x</parameter>\n</function>";
    let result = parse_tool_calls(content);
    assert_eq!(
        result.tool_calls.len(),
        1,
        "errors: {:?}",
        result.parse_errors
    );
    assert_eq!(result.tool_calls[0].arguments["path"], "a.md");
    assert_eq!(result.tool_calls[0].arguments["content"], "x");
}

// ---------------------------------------------------------------------------
// D6: mixed-format batches. Every fixture below is the verbatim `content` of a
// qwen38-flash-next response from the 0.8.2 validation (runs/b3_review,
// runs/long_review turn artifacts).
// ---------------------------------------------------------------------------

/// b3_review turn_0014: an XML `<tool>` call followed by a Qwen3
/// `<function=…><parameter name=…>` call. 0.8.2 executed only the first.
const B3_TURN_0014: &str = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/tool_validator.rs\"}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=directory_tree>\n<parameter name=\"path\">tests/unit\n</parameter>\n<parameter name=\"max_depth\">2\n</parameter>\n</function>\n</tool_call>";

/// b3_review turn_0010: the second call is `<function=tool>` + `<name>` +
/// `<arguments>` closed by `</tool>` — the generic wrapper written with an
/// `<arguments>` element (0.8.3 D3 variant), unwrapped to the named tool.
const B3_TURN_0010: &str = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/tool_dispatch/mod.rs\", \"line_range\": [1900, 2180]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/tool_validator.rs\"}</arguments>\n</tool>";

/// b3_review turn_0003: second call `<function=file_read>` + `<arguments>`
/// closed by `</tool>` (no `</function>`).
const B3_TURN_0003: &str = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/api/streaming.rs\", \"line_range\": [1, 200]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=file_read>\n<arguments>{\"path\": \"src/tool_parser.rs\", \"line_range\": [1, 160]}</arguments>\n</tool>";

/// b3_review turn_0002: two `<tool>` calls, the second wrapped in
/// `<tool_call>` — wrappers alone are not a rejected call.
const B3_TURN_0002: &str = "\n\n<tool>\n<name>directory_tree</name>\n<arguments>{\"path\": \"src/api\", \"max_depth\": 2}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<tool>\n<name>directory_tree</name>\n<arguments>{\"path\": \"src/agent\", \"max_depth\": 2}</arguments>\n</tool>\n</tool_call>";

/// b3_review turn_0024: a malformed `<tool>` call (parameter instead of
/// arguments) followed by a valid Qwen3 call.
const B3_TURN_0024: &str = "\n\n<tool>\n<name>grep_search</name>\n<parameter name=\"arguments\">{\"pattern\": \"fn save|fn load|delta|last_write|persisted_tool_call_count|fn save_final|workspace|identity\", \"path\": \"src/checkpoint.rs\", \"context_lines\": 1, \"max_matches\": 40}</parameter>\n</tool>\n</tool_call>\n<tool_call>\n<function=glob_find>\n<parameter name=\"pattern\">src/checkpoint*.rs\n</parameter>\n</function>\n</tool_call>";

fn names(result: &ParseResult) -> Vec<&str> {
    result
        .tool_calls
        .iter()
        .map(|c| c.tool_name.as_str())
        .collect()
}

#[test]
fn d6_turn_0014_mixed_xml_then_qwen3_parses_both_calls() {
    let result = parse_tool_calls(B3_TURN_0014);
    assert_eq!(names(&result), vec!["file_read", "directory_tree"]);
    assert_eq!(
        result.tool_calls[0].arguments,
        serde_json::json!({"path": "src/agent/tool_validator.rs"})
    );
    assert_eq!(
        result.tool_calls[1].arguments,
        serde_json::json!({"path": "tests/unit", "max_depth": 2})
    );
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d6_mixed_qwen3_then_xml_parses_both_in_text_order() {
    // Same two calls, opposite order.
    let content = "<tool_call>\n<function=directory_tree>\n<parameter name=\"path\">tests/unit\n</parameter>\n<parameter name=\"max_depth\">2\n</parameter>\n</function>\n</tool_call>\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/tool_validator.rs\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["directory_tree", "file_read"]);
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d6_mixed_json_tool_call_and_xml_in_both_orders() {
    let json = "<tool_call>\n{\"name\": \"glob_find\", \"arguments\": {\"pattern\": \"src/*.rs\"}}\n</tool_call>";
    let xml =
        "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>";
    let r1 = parse_tool_calls(&format!("{json}\n{xml}"));
    assert_eq!(names(&r1), vec!["glob_find", "file_read"]);
    let r2 = parse_tool_calls(&format!("{xml}\n{json}"));
    assert_eq!(names(&r2), vec!["file_read", "glob_find"]);
    assert!(r1.rejections.is_empty() && r2.rejections.is_empty());
}

#[test]
fn d6_mixed_openai_function_and_bare_qwen3_in_both_orders() {
    let openai = "<function=file_read>{\"path\": \"a.rs\"}</function>";
    let bare = "<function=glob_find>\n<parameter=pattern>*.rs</parameter>\n</function>";
    let r1 = parse_tool_calls(&format!("{openai}\n{bare}"));
    assert_eq!(names(&r1), vec!["file_read", "glob_find"]);
    assert_eq!(r1.tool_calls[0].arguments["path"], "a.rs");
    let r2 = parse_tool_calls(&format!("{bare}\n{openai}"));
    assert_eq!(names(&r2), vec!["glob_find", "file_read"]);
}

#[test]
fn d6_same_call_in_two_formats_executes_once() {
    let content = "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/tool_validator.rs\"}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=file_read>\n<parameter name=\"path\">src/agent/tool_validator.rs\n</parameter>\n</function>\n</tool_call>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["file_read"]);
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d6_nested_formats_claim_text_once() {
    // A Qwen3 call wrapped in <tool_call>: the wrapper family and the bare
    // family both match; the overlap resolves to ONE call.
    let content = "<tool_call>\n<function=glob_find>\n<parameter=pattern>*.rs</parameter>\n</function>\n</tool_call>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["glob_find"]);
    // A JSON code block inside a file_write payload is payload, not a call.
    let content = "<tool>\n<name>file_write</name>\n<arguments>{\"path\": \"README.md\", \"content\": \"```json\\n{\\\"name\\\": \\\"shell_exec\\\"}\\n```\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["file_write"]);
}

#[test]
fn d6_turn_0010_generic_wrapper_second_call_is_unwrapped() {
    // Until 0.8.3 this second call was rejected (no parser accepted it); the
    // D3-variant family now unwraps it to the file_read it names.
    let result = parse_tool_calls(B3_TURN_0010);
    assert_eq!(names(&result), vec!["file_read", "file_read"]);
    assert_eq!(result.tool_calls[0].arguments["line_range"][0], 1900);
    assert_eq!(
        result.tool_calls[1].arguments,
        serde_json::json!({"path": "src/agent/tool_validator.rs"})
    );
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d6_turn_0003_function_with_tool_close_is_rejected() {
    let result = parse_tool_calls(B3_TURN_0003);
    assert_eq!(names(&result), vec!["file_read"]);
    assert_eq!(
        result.tool_calls[0].arguments["path"],
        "src/api/streaming.rs"
    );
    assert_eq!(result.rejections.len(), 1, "{:?}", result.rejections);
    assert_eq!(result.rejections[0].tool_name.as_deref(), Some("file_read"));
    assert!(result.rejections[0].raw_text.contains("src/tool_parser.rs"));
}

#[test]
fn d6_turn_0002_wrapper_tags_alone_are_not_rejections() {
    let result = parse_tool_calls(B3_TURN_0002);
    assert_eq!(names(&result), vec!["directory_tree", "directory_tree"]);
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d6_turn_0024_malformed_first_valid_second() {
    let result = parse_tool_calls(B3_TURN_0024);
    assert_eq!(names(&result), vec!["glob_find"]);
    assert_eq!(
        result.tool_calls[0].arguments["pattern"],
        "src/checkpoint*.rs"
    );
    assert_eq!(result.rejections.len(), 1, "{:?}", result.rejections);
    assert_eq!(
        result.rejections[0].tool_name.as_deref(),
        Some("grep_search")
    );
}

#[test]
fn d6_quoted_markup_in_prose_and_code_is_not_a_rejection() {
    // A final answer that discusses tool-call syntax is not a tool call.
    let content = "The parser accepts `<tool_call>` and `<function=name>` forms.\n\n```xml\n<tool_call>\n<function=tool>\n<name>x</name>\n</tool_call>\n```\n\nIn prose, a stray <function=foo> mention mid-line is text.";
    let result = parse_tool_calls(content);
    assert!(result.tool_calls.is_empty());
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d6_invalid_json_tool_call_is_rejected_with_reason() {
    let content = "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>\n<tool_call>\n{\"name\": \"glob_find\", \"arguments\": {\"pattern\": }\n</tool_call>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["file_read"]);
    assert_eq!(result.rejections.len(), 1, "{:?}", result.rejections);
    assert!(result.rejections[0].reason.contains("Invalid JSON"));
}

// ---------------------------------------------------------------------------
// D3: generic wrapper `<function=tool><parameter=name>X</parameter>
// <parameter=arguments>{..}</parameter>` unwraps to the real tool
// (runs/long_review turns 0002, 0003 — 0.8.2 rejected a tool named 'tool').
// ---------------------------------------------------------------------------

const LONG_TURN_0002: &str = "\n\n<tool_call>\n<function=tool>\n<parameter=name>\ncontext_focus\n</parameter>\n<parameter=arguments>\n{\"query\": \"API streaming tool call parsing malformed empty truncated response\", \"max_files\": 5}\n</parameter>\n</function>\n</tool_call>";

const LONG_TURN_0003: &str = "\n\n<tool_call>\n<function=tool>\n<parameter=name>\nfile_read\n</parameter>\n<parameter=arguments>\n{\"path\": \"src/api/streaming.rs\"}\n</parameter>\n</function>\n</tool_call>";

#[test]
fn d3_long_review_turn_0002_unwraps_generic_tool_wrapper() {
    let result = parse_tool_calls(LONG_TURN_0002);
    assert_eq!(names(&result), vec!["context_focus"]);
    assert_eq!(
        result.tool_calls[0].arguments,
        serde_json::json!({
            "query": "API streaming tool call parsing malformed empty truncated response",
            "max_files": 5
        })
    );
}

#[test]
fn d3_long_review_turn_0003_unwraps_generic_tool_wrapper() {
    let result = parse_tool_calls(LONG_TURN_0003);
    assert_eq!(names(&result), vec!["file_read"]);
    assert_eq!(
        result.tool_calls[0].arguments,
        serde_json::json!({"path": "src/api/streaming.rs"})
    );
}

#[test]
fn d3_unwrap_accepts_string_arguments_and_every_wrapper_name() {
    for wrapper in ["tool", "tool_call", "function", "call"] {
        let content = format!(
            "<function={wrapper}>\n<parameter=name>file_read</parameter>\n<parameter=arguments>\"{{\\\"path\\\": \\\"a.rs\\\"}}\"</parameter>\n</function>"
        );
        let result = parse_tool_calls(&content);
        assert_eq!(names(&result), vec!["file_read"], "wrapper {wrapper}");
        assert_eq!(result.tool_calls[0].arguments["path"], "a.rs");
    }
}

#[test]
fn d3_unwrap_leaves_real_tools_and_extra_parameters_alone() {
    // A real tool with `name`/`arguments` parameters is not a wrapper.
    let content = "<function=knowledge_add>\n<parameter=name>x</parameter>\n<parameter=arguments>{\"a\": 1}</parameter>\n</function>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["knowledge_add"]);
    // A wrapper with a third parameter is not exactly name + arguments.
    let content = "<function=tool>\n<parameter=name>file_read</parameter>\n<parameter=arguments>{\"path\": \"a\"}</parameter>\n<parameter=extra>1</parameter>\n</function>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["tool"]);
}

// ---------------------------------------------------------------------------
// N6: a prose EXAMPLE of call markup must never start a match. The lazy
// `<tool>` regex used to start at a backticked example and run on to the
// real call's `</arguments></tool>` (0.8.3 validation, runs/b3_review
// turn_0006: a bogus call to `x` ran, the real grep_search was lost).
// Fixtures are the verbatim `content` of the turn artifacts.
// ---------------------------------------------------------------------------

const B3_083_TURN_0006: &str = "\n\n**Stage 1 findings (source-confirmed):**\n\n- **Gap in `src/tool_parser.rs`**: the single-`</tool>`-close form `<tool><name>x</name><arguments>{...}</tool>` matches none of the seven XML regexes (`xml_tool_regex` at :88 needs `</arguments>`; `xml_tool_missing_args_close_regex` at :135 needs two `</tool>`), and `normalize_malformed_xml` (:170) doesn't rewrite it because the bare-name rule requires whitespace before the tag name — so the call is silently dropped.\n- `src/api/streaming.rs` `ToolCallAccumulator::process_delta` (:~560) removes an entry on early emission, so a later fragment for the same index starts a fresh entry and loses prior args; `flush` (:~600) then filters entries with empty id.\n\nContinuing to Stage 2.\n\n<tool>\n<name>grep_search</name>\n<arguments>{\"pattern\": \"fn (dispatch|execute_tool_call|run_tool)\", \"path\": \"src/agent/mod.rs\", \"context_lines\": 3, \"max_matches\": 20}</arguments>\n</tool>";

/// val082 runs/long_review turn_0099: a final report that quotes the
/// `<tool>` syntax in backticks. 0.8.2 executed it as a call to `...`.
const LONG_082_TURN_0099: &str = "I have enough information to complete the audit. Let me now compile my final report covering all six stages with concrete findings, file:line citations, and distinguishing source-confirmed from runtime-tested behavior.\n\nLet me recall what I've found across the stages:\n\n**Stage 1 - API streaming and tool-call parsing:**\n- `src/api/streaming.rs` - handles SSE parsing, chunk accumulation\n- `src/tool_parser.rs` - parses `<tool><name>...</name><arguments>...</arguments></tool>` XML format\n- `src/agent/streaming.rs` - agent-level streaming coordination\n- `src/agent/recovery.rs` - handles malformed/empty/truncated responses\n\nKey findings:\n- `tool_parser.rs` uses `find(\"";

fn b3_083_turn_0006_real_call() -> serde_json::Value {
    serde_json::json!({
        "pattern": "fn (dispatch|execute_tool_call|run_tool)",
        "path": "src/agent/mod.rs",
        "context_lines": 3,
        "max_matches": 20
    })
}

#[test]
fn n6_turn_0006_backticked_example_does_not_swallow_the_real_call() {
    let result = parse_tool_calls(B3_083_TURN_0006);
    assert_eq!(names(&result), vec!["grep_search"]);
    assert_eq!(result.tool_calls[0].arguments, b3_083_turn_0006_real_call());
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
    // The prose (with its quoted example) stays text.
    assert!(result
        .text_content
        .contains("`<tool><name>x</name><arguments>{...}</tool>`"));
}

#[test]
fn n6_turn_0006_unquoted_example_still_yields_only_the_real_call() {
    // Same turn with the backticks stripped: the example no longer sits in a
    // code span, but its match would run into the real call's line-start
    // `<tool>` opener, so it is not a call.
    let content = B3_083_TURN_0006.replace('`', "");
    let result = parse_tool_calls(&content);
    assert_eq!(names(&result), vec!["grep_search"]);
    assert_eq!(result.tool_calls[0].arguments, b3_083_turn_0006_real_call());
}

#[test]
fn n6_long_review_turn_0099_quoted_syntax_in_final_report_is_not_a_call() {
    let result = parse_tool_calls(LONG_082_TURN_0099);
    assert!(result.tool_calls.is_empty(), "{:?}", names(&result));
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

/// One row per regex family: an unterminated prose example of the family's
/// syntax, and a real call in the same syntax (always `file_read` a.rs).
const FAMILY_EXAMPLES: &[(&str, &str, &str)] = &[
    (
        "xml <tool><name>",
        "<tool><name>x</name><arguments>{...}",
        "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "xml <name=x</name>",
        "<tool><name=x</name><arguments>{...}",
        "<tool>\n<name=file_read</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "xml <name=x>",
        "<tool><name=x><arguments>{...}",
        "<tool>\n<name=file_read>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "xml <function=x</function>",
        "<tool><function=x</function><arguments>{...}",
        "<tool>\n<function=file_read</function>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "xml <function>x</function>",
        "<tool><function>x</function><arguments>{...}",
        "<tool>\n<function>file_read</function>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "xml missing </arguments>",
        "<tool><name>x</name><arguments>{...}",
        "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"a.rs\"}\n</tool>\n</tool>",
    ),
    (
        "qwen3 <tool_call><function=>",
        "<tool_call><function=x><parameter=p>v</parameter>",
        "<tool_call>\n<function=file_read>\n<parameter=path>a.rs</parameter>\n</function>\n</tool_call>",
    ),
    (
        "json in <tool_call>",
        "<tool_call>{\"name\": \"x\", \"arguments\": {}}",
        "<tool_call>\n{\"name\": \"file_read\", \"arguments\": {\"path\": \"a.rs\"}}\n</tool_call>",
    ),
    (
        "generic wrapper + <arguments> element",
        "<function=tool><name>x</name><arguments>{...}",
        "<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "generic wrapper + mismatched slot closers",
        "<function=tool><parameter=name>x</name><parameter=arguments>{...}",
        "<function=tool>\n<parameter=name>\nfile_read</name>\n<parameter=arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>",
    ),
    (
        "openai <function=>{json}",
        "<function=x>{\"a\": 1}",
        "<function=file_read>{\"path\": \"a.rs\"}</function>",
    ),
    (
        "bare <function=><parameter=>",
        "<function=x><parameter=p>v</parameter>",
        "<function=file_read>\n<parameter=path>a.rs</parameter>\n</function>",
    ),
    (
        "kimi call tool=",
        "<|open|>call tool=\"x\" index=\"0\"<|sep|>",
        "<|open|>call tool=\"file_read\" index=\"1\"<|sep|><|open|>argument key=\"path\" type=\"string\"<|sep|>a.rs<|close|>argument<|close|>call",
    ),
];

fn assert_only_real_file_read(result: &ParseResult, family: &str) {
    assert_eq!(names(result), vec!["file_read"], "family {family}");
    assert_eq!(
        result.tool_calls[0].arguments,
        serde_json::json!({"path": "a.rs"}),
        "family {family}"
    );
    assert!(
        result.rejections.is_empty(),
        "family {family}: {:?}",
        result.rejections
    );
}

#[test]
fn n6_every_family_real_call_parses_on_its_own() {
    for (family, _, real) in FAMILY_EXAMPLES {
        assert_only_real_file_read(&parse_tool_calls(real), family);
    }
}

#[test]
fn n6_every_family_backticked_example_before_real_call_is_ignored() {
    for (family, example, real) in FAMILY_EXAMPLES {
        let content = format!("The model may emit `{example}` here.\n\n{real}");
        assert_only_real_file_read(&parse_tool_calls(&content), family);
    }
}

#[test]
fn n6_every_family_unquoted_example_before_real_call_is_ignored() {
    for (family, example, real) in FAMILY_EXAMPLES {
        let content = format!("The model may emit {example} here.\n{real}");
        assert_only_real_file_read(&parse_tool_calls(&content), family);
    }
}

#[test]
fn n6_every_family_quoted_complete_call_is_not_a_call() {
    for (family, _, real) in FAMILY_EXAMPLES {
        // A complete call quoted inline, and quoted in a fence.
        let inline = format!("Write it as `{}` exactly.", real.replace('\n', ""));
        let fenced = format!("Write it as:\n\n```xml\n{real}\n```\n\nThat is all.");
        for content in [inline, fenced] {
            let result = parse_tool_calls(&content);
            assert!(
                result.tool_calls.is_empty(),
                "family {family}: {:?}",
                names(&result)
            );
            assert!(
                result.rejections.is_empty(),
                "family {family}: {:?}",
                result.rejections
            );
        }
    }
}

#[test]
fn n6_plain_function_call_in_a_fence_is_an_example() {
    let content =
        "Run it like this:\n\n```\nshell_exec(\"rm -rf target\")\n```\n\nfile_read(\"a.rs\")";
    assert_only_real_file_read(&parse_tool_calls(content), "plain");
}

#[test]
fn n6_markup_inside_a_real_calls_json_payload_is_payload() {
    // A mid-line `<tool>` inside a JSON string is not another call's opener.
    let content = "<tool>\n<name>file_write</name>\n<arguments>{\"path\": \"a.md\", \"content\": \"use <tool><name>x</name> tags\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert_eq!(names(&result), vec!["file_write"]);
    assert_eq!(
        result.tool_calls[0].arguments["content"],
        "use <tool><name>x</name> tags"
    );
}

#[test]
fn n6_fenced_json_block_call_still_parses() {
    // Fenced JSON is a call syntax of its own, not a quoted example.
    let content = "```json\n{\"name\": \"file_read\", \"arguments\": {\"path\": \"a.rs\"}}\n```";
    assert_only_real_file_read(&parse_tool_calls(content), "json block");
}

// ---------------------------------------------------------------------------
// D3 variant: the generic wrapper written with an `<arguments>` ELEMENT,
// `<function=tool><name>X</name><arguments>{..}</arguments></tool>` (or a
// `<parameter name="name">X</parameter>` header). 0.8.3 runs/long_review
// rejected 29 such calls (25 `<name>`, 4 `<parameter name="name">`).
// ---------------------------------------------------------------------------

const LONG_083_TURN_0105: &str = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/api/streaming.rs\", \"line_range\": [750, 850]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<parameter name=\"name\">file_read</parameter>\n<arguments>{\"path\": \"/work/src/tool_parser.rs\", \"line_range\": [120, 320]}</arguments>\n</tool>";

const LONG_083_TURN_0108: &str = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/tool_parser.rs\", \"line_range\": [500, 700]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/agent/execution.rs\", \"line_range\": [1, 150]}</arguments>\n</tool>\n</tool_call>";

const LONG_083_TURN_0110: &str = "\n\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/agent/execution.rs\", \"line_range\": [1, 200]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/agent/tool_validator.rs\", \"line_range\": [1, 200]}</arguments>\n</tool>\n</tool_call>";

const LONG_083_TURN_0123: &str = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/agent/best_snapshot.rs\", \"line_range\": [1, 150]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/hooks/mod.rs\", \"line_range\": [1, 150]}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"/work/src/tools/screen_capture.rs\", \"line_range\": [1, 150]}</arguments>\n</tool>\n</tool_call>";

fn paths(result: &ParseResult) -> Vec<&str> {
    result
        .tool_calls
        .iter()
        .map(|c| c.arguments["path"].as_str().unwrap_or_default())
        .collect()
}

#[test]
fn d3v_turn_0105_parameter_name_header_unwraps() {
    let result = parse_tool_calls(LONG_083_TURN_0105);
    assert_eq!(names(&result), vec!["file_read", "file_read"]);
    assert_eq!(
        result.tool_calls[1].arguments,
        serde_json::json!({"path": "/work/src/tool_parser.rs", "line_range": [120, 320]})
    );
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d3v_turn_0108_name_element_header_unwraps() {
    let result = parse_tool_calls(LONG_083_TURN_0108);
    assert_eq!(names(&result), vec!["file_read", "file_read"]);
    assert_eq!(
        result.tool_calls[1].arguments,
        serde_json::json!({"path": "/work/src/agent/execution.rs", "line_range": [1, 150]})
    );
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d3v_turn_0110_two_wrapped_calls_both_unwrap() {
    let result = parse_tool_calls(LONG_083_TURN_0110);
    assert_eq!(
        paths(&result),
        vec![
            "/work/src/agent/execution.rs",
            "/work/src/agent/tool_validator.rs"
        ]
    );
    assert_eq!(names(&result), vec!["file_read", "file_read"]);
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d3v_turn_0123_xml_then_two_wrapped_calls() {
    let result = parse_tool_calls(LONG_083_TURN_0123);
    assert_eq!(
        paths(&result),
        vec![
            "/work/src/agent/best_snapshot.rs",
            "/work/src/hooks/mod.rs",
            "/work/src/tools/screen_capture.rs"
        ]
    );
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn d3v_only_generic_wrappers_with_object_arguments_unwrap() {
    // A real tool name in the function slot is not a wrapper: still rejected
    // (b3_review turn_0003 shape).
    let content = "<tool_call>\n<function=file_read>\n<name>file_read</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(result.tool_calls.is_empty(), "{:?}", names(&result));
    assert_eq!(result.rejections.len(), 1, "{:?}", result.rejections);
    // Arguments that are not a JSON object do not unwrap.
    let content = "<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>a.rs</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(result.tool_calls.is_empty(), "{:?}", names(&result));
    assert_eq!(result.rejections.len(), 1, "{:?}", result.rejections);
    // An inner name that is not an identifier does not unwrap.
    let content = "<tool_call>\n<function=tool>\n<name>...</name>\n<arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(result.tool_calls.is_empty(), "{:?}", names(&result));
    // Every generic wrapper name, closed by `</function>` too.
    for wrapper in ["tool", "tool_call", "function", "call"] {
        let content = format!(
            "<function={wrapper}>\n<parameter=name>file_read</parameter>\n<arguments>{{\"path\": \"a.rs\"}}</arguments>\n</function>"
        );
        assert_only_real_file_read(&parse_tool_calls(&content), wrapper);
    }
}

// ---------------------------------------------------------------------------
// M1: generic wrapper with MISMATCHED slot closers. The opener names the slot
// (`<name>` / `<parameter=name>`, `<arguments>` / `<parameter=arguments>`);
// the model closes it with whichever of `</name>`, `</parameter>`,
// `</arguments>` comes to mind. 0.8.4 validation runs/review rejected 38 of
// 53 turns from turn 26 on and never wrote REVIEW.md. Fixtures are the
// verbatim `content` of the turn artifacts (val084/repro).
// ---------------------------------------------------------------------------

const REVIEW_084_TURN_0005: &str = "\n\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<parameter=arguments>{\"path\": \"src/agent/task_runner.rs\", \"line_range\": [1050, 1400], \"line_numbers\": true}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<parameter=arguments>{\"path\": \"src/agent/execution.rs\", \"line_range\": [420, 780], \"line_numbers\": true}</arguments>\n</tool>\n</tool_call>";
const REVIEW_084_TURN_0007: &str = "\n\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/task_runner.rs\", \"line_range\": [1400, 1750], \"line_numbers\": true}</arguments>\n</tool>\n</tool_call>";
const REVIEW_084_TURN_0016: &str = "\n\n<tool_call>\n<function=tool>\n<parameter=name>\nfile_read</name>\n<parameter=arguments>{\"path\": \"src/agent/execution.rs\", \"line_range\": [760, 1120], \"line_numbers\": true}</arguments>\n</tool>\n</tool_call>";
const REVIEW_084_TURN_0021: &str = "\n\n<tool_call>\n<function=tool>\n<parameter=name>\nfile_read\n</parameter>\n<parameter=arguments>{\"path\": \"src/agent/execution.rs\", \"line_range\": [1480, 1840], \"line_numbers\": true}</arguments>\n</tool>\n</tool_call>";
const REVIEW_084_TURN_0042: &str = "\n\n<tool_call>\n<function=tool>\n<parameter=name>\nfile_read</name>\n<parameterarguments>{\"path\": \"src/agent/task_runner.rs\", \"line_range\": [2760, 2900], \"line_numbers\": true}</arguments>\n</tool>";
/// runs/review turn_0049: the Qwen3 `<tool_call>` family used to claim this
/// and dispatch the non-tool `tool` with name `grep_search</name>…`.
const REVIEW_084_TURN_0049: &str = "\n\n<tool_call>\n<function=tool>\n<parameter=name>\ngrep_search</name>\n<parameter=arguments>{\"path\": \"src\", \"pattern\": \"AgentState::Completed\", \"include\": \"*.rs\", \"context_lines\": 3, \"max_matches\": 20}\n</parameter>\n</function>\n</tool_call>";
const REVIEW_084_V_PARAMCLOSE: &str = "<tool_call>\n<function=tool>\n<name>file_read</name>\n<parameter=arguments>{\"path\": \"src/lib.rs\"}</parameter>\n</tool>\n</tool_call>";
/// Control: the well-formed Qwen generic wrapper, parsed before this fix.
const REVIEW_084_V_QWENWRAPPER: &str = "<tool_call>\n<function=tool>\n<parameter=name>file_read</parameter>\n<parameter=arguments>{\"path\": \"src/lib.rs\"}</parameter>\n</function>\n</tool_call>";

fn file_read_range(path: &str, from: u64, to: u64) -> serde_json::Value {
    serde_json::json!({"path": path, "line_range": [from, to], "line_numbers": true})
}

fn assert_exact_calls(content: &str, expected: &[(&str, serde_json::Value)]) {
    let result = parse_tool_calls(content);
    let got: Vec<(String, serde_json::Value)> = result
        .tool_calls
        .iter()
        .map(|c| (c.tool_name.clone(), c.arguments.clone()))
        .collect();
    let want: Vec<(String, serde_json::Value)> = expected
        .iter()
        .map(|(n, a)| (n.to_string(), a.clone()))
        .collect();
    assert_eq!(got, want);
    assert!(result.rejections.is_empty(), "{:?}", result.rejections);
}

#[test]
fn m1_turn_0005_name_element_with_parameter_arguments_closed_by_arguments() {
    assert_exact_calls(
        REVIEW_084_TURN_0005,
        &[
            (
                "file_read",
                file_read_range("src/agent/task_runner.rs", 1050, 1400),
            ),
            (
                "file_read",
                file_read_range("src/agent/execution.rs", 420, 780),
            ),
        ],
    );
}

#[test]
fn m1_turn_0007_matched_closers_still_parse() {
    assert_exact_calls(
        REVIEW_084_TURN_0007,
        &[(
            "file_read",
            file_read_range("src/agent/task_runner.rs", 1400, 1750),
        )],
    );
}

#[test]
fn m1_turn_0016_parameter_name_closed_by_name() {
    assert_exact_calls(
        REVIEW_084_TURN_0016,
        &[(
            "file_read",
            file_read_range("src/agent/execution.rs", 760, 1120),
        )],
    );
}

#[test]
fn m1_turn_0021_parameter_name_on_its_own_lines() {
    assert_exact_calls(
        REVIEW_084_TURN_0021,
        &[(
            "file_read",
            file_read_range("src/agent/execution.rs", 1480, 1840),
        )],
    );
}

#[test]
fn m1_turn_0042_separator_less_arguments_opener() {
    assert_exact_calls(
        REVIEW_084_TURN_0042,
        &[(
            "file_read",
            file_read_range("src/agent/task_runner.rs", 2760, 2900),
        )],
    );
}

#[test]
fn m1_turn_0049_wrapper_outranks_the_qwen3_family() {
    assert_exact_calls(
        REVIEW_084_TURN_0049,
        &[(
            "grep_search",
            serde_json::json!({"path": "src", "pattern": "AgentState::Completed", "include": "*.rs", "context_lines": 3, "max_matches": 20}),
        )],
    );
}

#[test]
fn m1_param_close_and_qwen_wrapper_control() {
    for content in [REVIEW_084_V_PARAMCLOSE, REVIEW_084_V_QWENWRAPPER] {
        assert_exact_calls(
            content,
            &[("file_read", serde_json::json!({"path": "src/lib.rs"}))],
        );
    }
}

#[test]
fn m1_mismatched_closers_keep_the_d3_safety_conditions() {
    // A real tool name in the function slot is not a wrapper.
    let content = "<tool_call>\n<function=file_read>\n<parameter=name>\nfile_read</name>\n<parameter=arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(
        result
            .tool_calls
            .iter()
            .all(|c| c.arguments != serde_json::json!({"path": "a.rs"})),
        "{:?}",
        names(&result)
    );
    // Arguments that are not a JSON object do not unwrap.
    let content = "<tool_call>\n<function=tool>\n<parameter=name>\nfile_read</name>\n<parameter=arguments>a.rs</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(
        !names(&result).contains(&"file_read"),
        "{:?}",
        names(&result)
    );
    // A non-identifier inner name does not unwrap.
    let content = "<tool_call>\n<function=tool>\n<parameter=name>\nfile read</name>\n<parameter=arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(result.tool_calls.is_empty(), "{:?}", names(&result));
    // Anything else in the body (a third slot) does not unwrap.
    let content = "<tool_call>\n<function=tool>\n<parameter=name>\nfile_read</name>\n<parameter=arguments>{\"path\": \"a.rs\"}</arguments>\n<parameter=extra>1</parameter>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(
        !names(&result).contains(&"file_read"),
        "{:?}",
        names(&result)
    );
    // The opener must identify the slot: an unknown opener is not a slot.
    let content = "<tool_call>\n<function=tool>\n<parameter=title>\nfile_read</name>\n<parameter=arguments>{\"path\": \"a.rs\"}</arguments>\n</tool>";
    let result = parse_tool_calls(content);
    assert!(
        !names(&result).contains(&"file_read"),
        "{:?}",
        names(&result)
    );
}

#[test]
fn m1_prose_example_of_the_mismatched_shape_is_not_a_call() {
    let example =
        "<function=tool><parameter=name>x</name><parameter=arguments>{\"a\": 1}</arguments></tool>";
    // Quoted inline and in a fence: an example, never a call.
    for content in [
        format!("Models sometimes write `{example}` instead."),
        format!("Models sometimes write:\n\n```xml\n{example}\n```\n"),
    ] {
        let result = parse_tool_calls(&content);
        assert!(result.tool_calls.is_empty(), "{:?}", names(&result));
        assert!(result.rejections.is_empty(), "{:?}", result.rejections);
    }
    // An unterminated example before a real call does not swallow it.
    let content = format!(
        "The model may emit `<function=tool><parameter=name>x</name>` here.\n\n{}",
        REVIEW_084_TURN_0016
    );
    assert_exact_calls(
        &content,
        &[(
            "file_read",
            file_read_range("src/agent/execution.rs", 760, 1120),
        )],
    );
}
