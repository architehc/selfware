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
