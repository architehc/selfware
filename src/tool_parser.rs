//! Robust tool call parser with XML and JSON fallback
//!
//! Handles multiple formats for tool calls:
//! 1. Native function calling (tool_calls in response)
//! 2. XML-style <tool>...</tool> blocks
//! 3. JSON code blocks with tool schema
//! 4. Markdown code blocks with tool invocations

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// A parsed tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub raw_text: String,
    pub parse_method: ParseMethod,
}

/// How the tool call was parsed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ParseMethod {
    /// Native API function calling
    Native,
    /// XML-style `<tool>` tags
    Xml,
    /// JSON code block
    Json,
    /// Markdown with tool invocation
    Markdown,
}

/// Result of parsing content for tool calls
#[derive(Debug)]
pub struct ParseResult {
    /// Successfully parsed tool calls
    pub tool_calls: Vec<ParsedToolCall>,
    /// Any text content that wasn't part of tool calls
    pub text_content: String,
    /// Parsing errors encountered (non-fatal)
    pub parse_errors: Vec<String>,
}

// All regex patterns are compiled once and cached via OnceLock to avoid
// recompilation on each call to parse_tool_calls(). This is critical for
// performance since the parser may be called on every LLM response.
static XML_TOOL_REGEX: OnceLock<Regex> = OnceLock::new();
static JSON_BLOCK_REGEX: OnceLock<Regex> = OnceLock::new();

static XML_TOOL_ALT_REGEX: OnceLock<Regex> = OnceLock::new();
static XML_TOOL_ALT2_REGEX: OnceLock<Regex> = OnceLock::new();
static XML_TOOL_FUNCTION_REGEX: OnceLock<Regex> = OnceLock::new();
static XML_TOOL_FUNCTION_TAG_REGEX: OnceLock<Regex> = OnceLock::new();
static XML_TOOL_MISSING_ARGS_CLOSE_REGEX: OnceLock<Regex> = OnceLock::new();
static QWEN3_TOOL_CALL_REGEX: OnceLock<Regex> = OnceLock::new();
static QWEN3_PARAMETER_REGEX: OnceLock<Regex> = OnceLock::new();
static BARE_FUNCTION_REGEX: OnceLock<Regex> = OnceLock::new();
static OPENAI_FUNCTION_REGEX: OnceLock<Regex> = OnceLock::new();
static MALFORMED_CLOSE_TAG_REGEX: OnceLock<Regex> = OnceLock::new();
static JSON_STRING_REGEX: OnceLock<Regex> = OnceLock::new();

/// Cached regex for XML element parsing: `<tag>content</tag>`
/// Previously this was compiled on every call to `parse_xml_arguments`.
static XML_ELEMENT_REGEX: OnceLock<Regex> = OnceLock::new();

fn xml_tool_regex() -> &'static Regex {
    XML_TOOL_REGEX.get_or_init(|| {
        // Use a more robust pattern that captures everything between tags
        // The [\s\S]*? is used instead of .*? to match across newlines more reliably
        Regex::new(
            r"(?s)<tool>\s*<name>([^<]+)</name>\s*<arguments>([\s\S]*?)</arguments>\s*</tool>",
        )
        .expect("Invalid XML tool regex")
    })
}

/// Alternate XML format used by some models (e.g., Qwen3-Coder)
/// Format: <tool><name=tool_name</name><arguments>{...}</arguments></tool>
fn xml_tool_alt_regex() -> &'static Regex {
    XML_TOOL_ALT_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool>\s*<name=([^<>\s]+)\s*</name>\s*<arguments>([\s\S]*?)</arguments>\s*</tool>")
            .expect("Invalid XML tool alt regex")
    })
}

/// Second alternate XML format with closing angle bracket
/// Format: <tool><name=tool_name><arguments>{...}</arguments></tool>
fn xml_tool_alt2_regex() -> &'static Regex {
    XML_TOOL_ALT2_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool>\s*<name=([^<>\s]+)>\s*<arguments>([\s\S]*?)</arguments>\s*</tool>")
            .expect("Invalid XML tool alt2 regex")
    })
}

/// Function-style XML format used by some models
/// Format: <tool><function=tool_name</function><arguments>{...}</arguments></tool>
fn xml_tool_function_regex() -> &'static Regex {
    XML_TOOL_FUNCTION_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool>\s*<function=([^<>\s]+)\s*</function>\s*<arguments>([\s\S]*?)</arguments>\s*</tool>")
            .expect("Invalid XML tool function regex")
    })
}

/// Function tag XML format used by some models
/// Format: <tool><function>tool_name</function><arguments>{...}</arguments></tool>
fn xml_tool_function_tag_regex() -> &'static Regex {
    XML_TOOL_FUNCTION_TAG_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool>\s*<function>([^<]+)</function>\s*<arguments>([\s\S]*?)</arguments>\s*</tool>")
            .expect("Invalid XML tool function tag regex")
    })
}

/// Malformed XML seen from Qwen: `</arguments>` is omitted and the model emits
/// `</tool></tool>`. Recover the JSON payload between `<arguments>` and the
/// first closing `</tool>`.
fn xml_tool_missing_args_close_regex() -> &'static Regex {
    XML_TOOL_MISSING_ARGS_CLOSE_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool>\s*<name>([^<]+)</name>\s*<arguments>([\s\S]*?)</tool>\s*</tool>")
            .expect("Invalid XML missing args close regex")
    })
}

/// Qwen3 tool_call format
/// Format: <tool_call><function=name><parameter=key>value</parameter>...</function></tool_call>
fn qwen3_tool_call_regex() -> &'static Regex {
    QWEN3_TOOL_CALL_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool_call>\s*<function=([a-zA-Z_][a-zA-Z0-9_]*)>([\s\S]*?)</function>\s*</tool_call>")
            .expect("Invalid Qwen3 tool_call regex")
    })
}

/// Qwen3 parameter format
/// Format: <parameter=key>value</parameter>
fn qwen3_parameter_regex() -> &'static Regex {
    QWEN3_PARAMETER_REGEX.get_or_init(|| {
        Regex::new(r"<parameter=([a-zA-Z_][a-zA-Z0-9_]*)>\s*([\s\S]*?)\s*</parameter>")
            .expect("Invalid Qwen3 parameter regex")
    })
}

/// Bare function format (without tool_call wrapper)
/// Format: <function=name><parameter=key>value</parameter>...</function>
fn bare_function_regex() -> &'static Regex {
    BARE_FUNCTION_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<function=([a-zA-Z_][a-zA-Z0-9_]*)>\s*([\s\S]*?)\s*</function>")
            .expect("Invalid bare function regex")
    })
}

/// OpenAI function calling format (without `<tool>` wrapper)
/// Format: <function=name>{"json":"args"}</function>
/// This is the format used by OpenAI-compatible endpoints that output function
/// calls with inline JSON arguments rather than `<parameter>` tags.
fn openai_function_regex() -> &'static Regex {
    OPENAI_FUNCTION_REGEX.get_or_init(|| {
        Regex::new(r#"(?s)<function=([a-zA-Z_][a-zA-Z0-9_]*)>\s*(\{[\s\S]*?\})\s*</function>"#)
            .expect("Invalid OpenAI function regex")
    })
}

/// Cached regex for parsing XML elements: `<tag>content</tag>`
/// Used by `parse_xml_arguments` to extract key-value pairs from XML-style arguments.
fn xml_element_regex() -> &'static Regex {
    XML_ELEMENT_REGEX.get_or_init(|| {
        Regex::new(r"<([a-zA-Z_][a-zA-Z0-9_]*)>([^<]*)</([a-zA-Z_][a-zA-Z0-9_]*)>")
            .expect("Invalid XML element regex")
    })
}

fn json_block_regex() -> &'static Regex {
    JSON_BLOCK_REGEX.get_or_init(|| {
        Regex::new(r"(?s)```(?:json)?\s*(\{[^`]*\})\s*```").expect("Invalid JSON block regex")
    })
}

/// Maximum input size for the tool parser (10 MB).
/// Inputs larger than this are truncated to prevent pathological regex performance.
const MAX_TOOL_PARSER_INPUT_SIZE: usize = 10 * 1024 * 1024;

/// Decode standard XML entities in a string.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Normalize malformed XML closing tags generated by some models.
///
/// Some models (e.g., Qwen3.5-4B) emit `arguments>` instead of `</arguments>`,
/// dropping the `</` prefix on closing tags. This function detects known tag names
/// that appear bare (not preceded by `<` or `/` or another word character) followed
/// by `>`, and rewrites them as proper closing tags before the regex parsers run.
///
/// JSON string literals are temporarily protected so that valid JSON inside
/// `<arguments>` blocks (e.g. `{"x": "name>"}`) is not corrupted by the rewrite.
fn normalize_malformed_xml(content: &str) -> String {
    let json_re = JSON_STRING_REGEX.get_or_init(|| {
        // Match a JSON string literal, including escaped quotes.
        Regex::new(r#""(?:[^"\\]|\\.)*""#).expect("Invalid JSON string regex")
    });
    let re = MALFORMED_CLOSE_TAG_REGEX.get_or_init(|| {
        // Match known closing-tag names followed by `>` only when the `>` is the last
        // non-whitespace token on a line or is immediately followed by another tag.
        // Require the tag name to be preceded by whitespace or line start so that `>`
        // inside JSON strings (e.g. `{"op": "a > b"}`) is not mistaken for a malformed
        // closing tag.
        Regex::new(r"(?m)(^|\s)(tool_call|arguments|parameter|function|tool|name)>(\s*(?:$|<))")
            .expect("Invalid malformed close tag regex")
    });

    // Protect JSON string literals so that valid JSON inside <arguments> blocks
    // (e.g. `{"x": "name>"}`) is not corrupted by the malformed-XML closing-tag
    // rewrite. The malformed tags we want to fix are XML envelope tags, not JSON
    // string values.
    let mut protected: Vec<String> = Vec::new();
    const PLACEHOLDER: &str = "\x00__JSON_STRING__\x00";

    let protected_content = json_re
        .replace_all(content, |caps: &regex::Captures| {
            protected.push(caps[0].to_string());
            PLACEHOLDER
        })
        .to_string();

    let normalized = re.replace_all(&protected_content, "$1</$2>$3").to_string();

    let mut result = normalized;
    for s in protected {
        result = result.replacen(PLACEHOLDER, &s, 1);
    }
    result
}

/// Parse content for tool calls using multiple strategies
pub fn parse_tool_calls(content: &str) -> ParseResult {
    // Enforce maximum input size to prevent pathological regex performance.
    // Use char-boundary-safe truncation to avoid panics on multi-byte UTF-8.
    let content = if content.len() > MAX_TOOL_PARSER_INPUT_SIZE {
        // Find the largest index <= MAX_TOOL_PARSER_INPUT_SIZE that is a char boundary
        let mut end = MAX_TOOL_PARSER_INPUT_SIZE;
        while end > 0 && !content.is_char_boundary(end) {
            end -= 1;
        }
        &content[..end]
    } else {
        content
    };

    // Fix malformed closing tags (e.g., `arguments>` → `</arguments>`)
    // before running the regex parsers.
    let content = normalize_malformed_xml(content);
    let content = content.as_str();

    let mut result = ParseResult {
        tool_calls: Vec::new(),
        text_content: content.to_string(),
        parse_errors: Vec::new(),
    };

    // Warn about unclosed tool tags (common with Qwen3.5 quantized models)
    if content.contains("<tool>") && !content.contains("</tool>") {
        tracing::warn!(
            "Unclosed <tool> tag detected — tool call may be lost. Content preview: {}",
            &content[..content.len().min(300)]
        );
    }

    // Strategy 1: Try XML-style parsing first (most common for our agent)
    if let Some(xml_results) = try_parse_xml(content) {
        for (tool_call, raw) in xml_results {
            match tool_call {
                Ok(tc) => {
                    // Remove the raw XML from text content
                    result.text_content = result.text_content.replace(&raw, "");
                    result.tool_calls.push(tc);
                }
                Err(e) => {
                    result.parse_errors.push(format!("XML parse error: {}", e));
                }
            }
        }
    }

    // Strategy 2: Try JSON code blocks if no XML found
    if result.tool_calls.is_empty() {
        if let Some(json_results) = try_parse_json_blocks(content) {
            for (tool_call, raw) in json_results {
                match tool_call {
                    Ok(tc) => {
                        result.text_content = result.text_content.replace(&raw, "");
                        result.tool_calls.push(tc);
                    }
                    Err(e) => {
                        result.parse_errors.push(format!("JSON parse error: {}", e));
                    }
                }
            }
        }
    }

    // Strategy 3: Try plain function-call syntax as last resort
    // Models sometimes output tool_name("arg1", "arg2") or tool_name(json) without XML tags
    if result.tool_calls.is_empty() {
        if let Some(func_results) = try_parse_plain_function_calls(content) {
            for (tool_call, raw) in func_results {
                match tool_call {
                    Ok(tc) => {
                        result.text_content = result.text_content.replace(&raw, "");
                        result.tool_calls.push(tc);
                    }
                    Err(e) => {
                        result
                            .parse_errors
                            .push(format!("Plain function parse error: {}", e));
                    }
                }
            }
        }
    }

    // Clean up text content
    result.text_content = result.text_content.trim().to_string();

    result
}

/// Try to parse XML-style tool calls
/// Supports both standard format and Qwen3-style format
fn try_parse_xml(content: &str) -> Option<Vec<(Result<ParsedToolCall>, String)>> {
    let regex = xml_tool_regex();
    let alt_regex = xml_tool_alt_regex();

    // Try standard format first
    let mut results: Vec<_> = regex
        .captures_iter(content)
        .map(|cap| {
            let raw = cap[0].to_string();
            let name = cap[1].trim().to_string();
            let args_str = cap[2].trim();

            let result = parse_xml_arguments(args_str).map(|arguments| ParsedToolCall {
                tool_name: name,
                arguments,
                raw_text: raw.clone(),
                parse_method: ParseMethod::Xml,
            });

            (result, raw)
        })
        .collect();

    // If no matches, try alternate format (Qwen3-style: <name=tool_name</name>)
    if results.is_empty() {
        results = alt_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let args_str = cap[2].trim();

                let result = parse_xml_arguments(args_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    // If still no matches, try second alternate format (<name=tool_name>)
    if results.is_empty() {
        let alt2_regex = xml_tool_alt2_regex();
        results = alt2_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let args_str = cap[2].trim();

                let result = parse_xml_arguments(args_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    // If still no matches, try function-style format (<function=tool_name</function>)
    if results.is_empty() {
        let func_regex = xml_tool_function_regex();
        results = func_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let args_str = cap[2].trim();

                let result = parse_xml_arguments(args_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    // If still no matches, try function tag format (<function>tool_name</function>)
    if results.is_empty() {
        let func_tag_regex = xml_tool_function_tag_regex();
        results = func_tag_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let args_str = cap[2].trim();

                let result = parse_xml_arguments(args_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    // If still no matches, recover Qwen's missing </arguments> variant:
    // <tool><name>x</name><arguments>{...}</tool></tool>
    if results.is_empty() {
        let malformed_regex = xml_tool_missing_args_close_regex();
        results = malformed_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let args_str = cap[2].trim();

                let result = parse_xml_arguments(args_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    // If still no matches, try Qwen3 tool_call format
    // Format: <tool_call><function=name><parameter=key>value</parameter>...</function></tool_call>
    if results.is_empty() {
        let qwen3_regex = qwen3_tool_call_regex();
        results = qwen3_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let params_str = &cap[2];

                let result = parse_qwen3_parameters(params_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    // Try <tool_call> with inline JSON format (Qwen3.5 122B / sglang)
    // Format: <tool_call>\n{"name": "tool", "arguments": {...}}\n</tool_call>
    if results.is_empty() {
        static TOOL_CALL_JSON_REGEX: once_cell::sync::OnceCell<Regex> =
            once_cell::sync::OnceCell::new();
        let tc_regex = TOOL_CALL_JSON_REGEX.get_or_init(|| {
            Regex::new(r"(?s)<tool_call>\s*(\{.*?\})\s*</tool_call>")
                .expect("Invalid tool_call JSON regex")
        });

        results = tc_regex
            .captures_iter(content)
            .filter_map(|cap| {
                let raw = cap[0].to_string();
                let json_str = cap[1].trim();

                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(json) => {
                        let name = json
                            .get("name")
                            .or(json.get("tool"))
                            .or(json.get("function"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())?;
                        let arguments = json
                            .get("arguments")
                            .or(json.get("args"))
                            .or(json.get("parameters"))
                            .cloned()
                            .unwrap_or(serde_json::json!({}));

                        Some((
                            Ok(ParsedToolCall {
                                tool_name: name,
                                arguments,
                                raw_text: raw.clone(),
                                parse_method: ParseMethod::Json,
                            }),
                            raw,
                        ))
                    }
                    Err(e) => Some((
                        Err(anyhow::anyhow!("Invalid JSON in <tool_call>: {}", e)),
                        raw,
                    )),
                }
            })
            .collect();
    }

    // If still no matches, try OpenAI function format with inline JSON
    // Format: <function=name>{"key": "value"}</function>
    // This MUST come before the bare function regex because both share the
    // `<function=name>...</function>` structure, but this variant carries
    // inline JSON while the bare variant uses `<parameter>` tags.
    if results.is_empty() {
        let openai_regex = openai_function_regex();
        results = openai_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let json_str = cap[2].trim();

                let result = serde_json::from_str::<serde_json::Value>(json_str)
                    .map(|arguments| ParsedToolCall {
                        tool_name: name,
                        arguments,
                        raw_text: raw.clone(),
                        parse_method: ParseMethod::Xml,
                    })
                    .map_err(|e| anyhow::anyhow!("Invalid JSON in OpenAI function call: {}", e));

                (result, raw)
            })
            .collect();
    }

    // If still no matches, try bare function format (without tool_call wrapper)
    // Format: <function=name><parameter=key>value</parameter>...</function>
    if results.is_empty() {
        let bare_func_regex = bare_function_regex();
        results = bare_func_regex
            .captures_iter(content)
            .map(|cap| {
                let raw = cap[0].to_string();
                let name = cap[1].trim().to_string();
                let params_str = &cap[2];

                let result = parse_qwen3_parameters(params_str).map(|arguments| ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw.clone(),
                    parse_method: ParseMethod::Xml,
                });

                (result, raw)
            })
            .collect();
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Parse Qwen3-style parameters: <parameter=key>value</parameter>
fn parse_qwen3_parameters(params_str: &str) -> Result<serde_json::Value> {
    let param_regex = qwen3_parameter_regex();
    let mut args = serde_json::Map::new();

    for cap in param_regex.captures_iter(params_str) {
        let key = cap[1].trim().to_string();
        let raw_value = cap[2].trim();
        let value = decode_xml_entities(raw_value);

        // Try to parse value as JSON (for booleans, numbers, arrays, objects)
        let json_value = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&value) {
            v
        } else {
            // Treat as string
            serde_json::Value::String(value.to_string())
        };

        args.insert(key, json_value);
    }

    if args.is_empty() {
        // Return empty object if no parameters found
        Ok(serde_json::json!({}))
    } else {
        Ok(serde_json::Value::Object(args))
    }
}

/// Extract a balanced JSON object from a string by counting braces.
///
/// Finds the first `{` in `s`, then tracks `{`/`}` depth while respecting
/// JSON string literals (and escape sequences like `\"` and `\\` inside them).
/// Returns the complete JSON substring and the byte index one past the closing `}`,
/// or `None` if braces never balance.
fn extract_json_balanced(s: &str) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let start = s.find('{')?;
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut i = start;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                // Skip the escaped character
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = i + 1;
                        return Some((&s[start..end], end));
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    None
}

/// Parse arguments from XML format (can be JSON or XML elements)
fn parse_xml_arguments(args_str: &str) -> Result<serde_json::Value> {
    let trimmed = args_str.trim();

    // First try: extract a balanced JSON object via brace counting, then parse.
    // This handles cases where HTML tags inside JSON string values would confuse
    // the regex-based capture (e.g., `{"content":"<div>hello</div>"}`).
    if let Some((json_str, _end)) = extract_json_balanced(trimmed) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
            return Ok(json);
        }
    }

    // Second try: parse as JSON directly (handles the simple/clean case)
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(json);
    }

    // Third try: parse as XML elements and convert to JSON
    let mut args = serde_json::Map::new();

    // XML element parser: <key>value</key>
    // Uses a cached (OnceLock) compiled regex to avoid recompilation on each call.
    let elem_regex = xml_element_regex();

    for cap in elem_regex.captures_iter(trimmed) {
        let open_tag = &cap[1];
        let raw_value = cap[2].trim();
        let value = decode_xml_entities(raw_value);
        let close_tag = &cap[3];

        // Only accept if tags match
        if open_tag == close_tag {
            let key = open_tag.to_string();

            // Try to parse value as JSON (for booleans, numbers, etc.)
            let json_value = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&value) {
                v
            } else {
                serde_json::Value::String(value.to_string())
            };

            args.insert(key, json_value);
        }
    }

    if args.is_empty() {
        // Last resort: treat the whole thing as a string argument
        Ok(serde_json::json!({"input": trimmed}))
    } else {
        Ok(serde_json::Value::Object(args))
    }
}

/// Try to parse JSON code blocks as tool calls
fn try_parse_json_blocks(content: &str) -> Option<Vec<(Result<ParsedToolCall>, String)>> {
    let regex = json_block_regex();

    if !regex.is_match(content) {
        return None;
    }

    let results: Vec<_> = regex
        .captures_iter(content)
        .filter_map(|cap| {
            let raw = cap[0].to_string();
            let json_str = &cap[1];

            // Try to parse as a tool call structure
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(json) => {
                    // Check if it looks like a tool call
                    if let Some(name) = json
                        .get("tool")
                        .or(json.get("name"))
                        .or(json.get("function"))
                    {
                        let tool_name = name.as_str()?.to_string();
                        let arguments = json
                            .get("arguments")
                            .or(json.get("args"))
                            .or(json.get("parameters"))
                            .cloned()
                            .unwrap_or(serde_json::json!({}));

                        Some((
                            Ok(ParsedToolCall {
                                tool_name,
                                arguments,
                                raw_text: raw.clone(),
                                parse_method: ParseMethod::Json,
                            }),
                            raw,
                        ))
                    } else {
                        None
                    }
                }
                Err(e) => Some((Err(anyhow::anyhow!("Invalid JSON: {}", e)), raw)),
            }
        })
        .collect();

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Try to parse plain function-call syntax from model output.
/// Matches patterns like:
///   file_read("src/main.rs")
///   file_edit("path", "old_str", "new_str")
///   shell_exec("cargo test")
///   tool_name({"key": "value"})
///
/// This is a last-resort fallback for models that don't wrap tool calls in XML tags.
fn try_parse_plain_function_calls(content: &str) -> Option<Vec<(Result<ParsedToolCall>, String)>> {
    // Known tool name prefixes — we only match calls that look like real tools
    const KNOWN_TOOLS: &[&str] = &[
        "file_read",
        "file_write",
        "file_edit",
        "file_multi_edit",
        "file_fim_edit",
        "file_delete",
        "directory_tree",
        "shell_exec",
        "grep_search",
        "glob_find",
        "symbol_search",
        "cargo_check",
        "cargo_test",
        "cargo_clippy",
        "cargo_fmt",
        "git_status",
        "git_diff",
        "git_commit",
        "git_push",
        "git_log",
        "git_checkpoint",
        "tool_search",
        "context_bulk_read",
        // Container / process tools
        "container_run",
        "container_stop",
        "container_list",
        "container_logs",
        "container_exec",
        "container_build",
        "container_images",
        "container_pull",
        "container_remove",
        "compose_up",
        "compose_down",
        "process_start",
        "process_stop",
        "process_list",
        "process_logs",
        "process_restart",
        "port_check",
        // LSP tools
        "lsp_goto_definition",
        "lsp_find_references",
        "lsp_document_symbols",
        "lsp_hover",
        "lsp_diagnostics",
        "lsp_workspace_symbols",
        "lsp_goto_implementation",
        // MCP / browser / computer tools
        "browser_fetch",
        "browser_screenshot",
        "browser_pdf",
        "browser_eval",
        "browser_links",
        "page_control",
        "computer_mouse",
        "computer_keyboard",
        "computer_screen",
        "computer_window",
        "screen_capture",
        "vision_analyze",
        "vision_compare",
        // Misc tools
        "patch_apply",
        "pty_shell",
        "http_request",
        "code_metrics",
        "code_map",
        "context_budget",
        "context_action",
        "code_introspect",
        "code_query",
        "code_plan",
        "code_diff_plan",
        "localize_issue",
        "npm_install",
        "npm_run",
        "npm_scripts",
        "pip_install",
        "pip_list",
        "pip_freeze",
        "yarn_install",
        "enter_worktree",
        "exit_worktree",
        "list_worktrees",
        "knowledge_add",
        "knowledge_relate",
        "knowledge_query",
        "knowledge_stats",
        "knowledge_clear",
        "knowledge_remove",
        "knowledge_export",
        "knowledge_auto_extract",
    ];

    static FUNC_CALL_REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = FUNC_CALL_REGEX.get_or_init(|| {
        // Match tool_name( ... ) where the parens can contain strings, JSON, etc.
        // Use a simple balanced-paren matcher for the arguments.
        Regex::new(r"(?m)^([a-z_]+)\((.+)\)\s*$").expect("Invalid function call regex")
    });

    let mut results = Vec::new();

    for cap in regex.captures_iter(content) {
        let raw = cap[0].to_string();
        let name = cap[1].to_string();
        let args_raw = cap[2].trim();

        if !KNOWN_TOOLS.contains(&name.as_str()) {
            continue;
        }

        // Try to parse arguments:
        // 1. If it's JSON object directly: file_read({"path": "src/main.rs"})
        // 2. If it's a quoted string: file_read("src/main.rs") → {"path": "src/main.rs"}
        // 3. If it's multiple quoted strings: file_edit("path", "old", "new")
        let arguments = if args_raw.starts_with('{') {
            // Direct JSON
            match serde_json::from_str::<serde_json::Value>(args_raw) {
                Ok(v) => v,
                Err(_) => continue,
            }
        } else {
            // Try to map positional args to known parameter names
            let positional = parse_positional_args(args_raw);
            if positional.is_empty() {
                continue;
            }
            match name.as_str() {
                "file_read" | "directory_tree" => {
                    serde_json::json!({"path": positional[0]})
                }
                "file_write" if positional.len() >= 2 => {
                    serde_json::json!({"path": positional[0], "content": positional[1]})
                }
                "file_edit" if positional.len() >= 3 => {
                    serde_json::json!({
                        "path": positional[0],
                        "old_str": positional[1],
                        "new_str": positional[2]
                    })
                }
                "shell_exec" | "cargo_check" | "cargo_test" | "cargo_clippy" | "cargo_fmt" => {
                    serde_json::json!({"command": positional[0]})
                }
                "grep_search" => {
                    if positional.len() >= 2 {
                        serde_json::json!({"pattern": positional[0], "path": positional[1]})
                    } else {
                        serde_json::json!({"pattern": positional[0]})
                    }
                }
                "glob_find" => {
                    serde_json::json!({"pattern": positional[0]})
                }
                "tool_search" => {
                    serde_json::json!({"query": positional[0]})
                }
                _ => {
                    // Generic: first arg as the first schema field
                    serde_json::json!({"input": positional[0]})
                }
            }
        };

        tracing::debug!(
            "Parsed plain function call: {}({}) → {}",
            name,
            args_raw,
            arguments
        );

        results.push((
            Ok(ParsedToolCall {
                tool_name: name,
                arguments,
                raw_text: raw.clone(),
                parse_method: ParseMethod::Json,
            }),
            raw,
        ));
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Parse positional arguments from a function call.
/// Handles: "arg1", "arg2", "arg3" and 'arg1', 'arg2'
fn parse_positional_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';

    while let Some(&ch) = chars.peek() {
        chars.next();
        if !in_quote {
            if ch == '"' || ch == '\'' {
                in_quote = true;
                quote_char = ch;
                current.clear();
            } else if ch == ',' {
                // skip comma between args
            }
        } else if ch == quote_char {
            args.push(current.clone());
            current.clear();
            in_quote = false;
        } else if ch == '\\' {
            // Handle escape sequences
            if let Some(&next) = chars.peek() {
                chars.next();
                match next {
                    'n' => current.push('\n'),
                    't' => current.push('\t'),
                    '\\' => current.push('\\'),
                    c if c == quote_char => current.push(c),
                    _ => {
                        current.push('\\');
                        current.push(next);
                    }
                }
            }
        } else {
            current.push(ch);
        }
    }

    args
}

/// Validate that a parsed tool call has the required structure
pub fn validate_tool_call(tool_call: &ParsedToolCall, available_tools: &[&str]) -> Result<()> {
    // Check tool exists
    if !available_tools.contains(&tool_call.tool_name.as_str()) {
        anyhow::bail!(
            "Unknown tool '{}'. Available tools: {:?}",
            tool_call.tool_name,
            available_tools
        );
    }

    // Arguments must be an object
    if !tool_call.arguments.is_object() {
        anyhow::bail!(
            "Tool arguments must be a JSON object, got: {}",
            tool_call.arguments
        );
    }

    Ok(())
}

/// Extract just the text content from a response, removing tool calls
pub fn extract_text_only(content: &str) -> String {
    let result = parse_tool_calls(content);
    result.text_content
}

#[cfg(test)]
#[path = "../tests/unit/tool_parser/tool_parser_tests_test.rs"]
mod tests;
