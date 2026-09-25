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
    /// Tool-call text that was NOT executed because no parser accepted it
    /// (malformed or mixed syntax, invalid JSON). Callers must tell the model.
    pub rejections: Vec<ParseRejection>,
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
static QWEN3_PARAMETER_OPEN_REGEX: OnceLock<Regex> = OnceLock::new();
static BARE_FUNCTION_REGEX: OnceLock<Regex> = OnceLock::new();
static OPENAI_FUNCTION_REGEX: OnceLock<Regex> = OnceLock::new();
static MALFORMED_CLOSE_TAG_REGEX: OnceLock<Regex> = OnceLock::new();
static JSON_STRING_REGEX: OnceLock<Regex> = OnceLock::new();
static KIMI_CALL_REGEX: OnceLock<Regex> = OnceLock::new();
static KIMI_ARG_REGEX: OnceLock<Regex> = OnceLock::new();

fn kimi_call_regex() -> &'static Regex {
    KIMI_CALL_REGEX.get_or_init(|| {
        Regex::new(r#"(?s)(?:<\|open\|>)?call\s+tool="([^"]+)"[^>]*<\|sep\|>(.*?)<\|close\|>call(?:<\|sep\|>)?"#)
            .expect("Invalid Kimi call regex")
    })
}

fn kimi_arg_regex() -> &'static Regex {
    KIMI_ARG_REGEX.get_or_init(|| {
        Regex::new(r#"(?s)<\|open\|>argument\s+key="([^"]+)"(?:\s+type="([^"]+)")?<\|sep\|>(.*?)<\|close\|>argument(?:<\|sep\|>)?"#)
            .expect("Invalid Kimi arg regex")
    })
}

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

/// Opening tag of a Qwen3 parameter, in every dialect models emit:
/// `<parameter=key>`, `<parameter = key>`, `<parameter="key">`,
/// `<parameter name="key">`, `<parameter name='key'>`, `<parameter name=key>`
/// (whitespace-tolerant). The key lands in exactly one of groups 1-3.
const QWEN3_PARAMETER_OPEN: &str = r#"<parameter(?:\s*=\s*|\s+name\s*=\s*)(?:"([a-zA-Z_][a-zA-Z0-9_]*)"|'([a-zA-Z_][a-zA-Z0-9_]*)'|([a-zA-Z_][a-zA-Z0-9_]*))\s*>"#;

/// Qwen3 parameter format
/// Format: `<parameter=key>value</parameter>` (or any dialect of
/// [`QWEN3_PARAMETER_OPEN`]); the value is group 4.
fn qwen3_parameter_regex() -> &'static Regex {
    QWEN3_PARAMETER_REGEX.get_or_init(|| {
        Regex::new(&format!(
            r"{QWEN3_PARAMETER_OPEN}\s*([\s\S]*?)\s*</parameter>"
        ))
        .expect("Invalid Qwen3 parameter regex")
    })
}

/// Whether a call body carries any Qwen3 parameter tag (any dialect).
fn has_qwen3_parameter_tag(params_str: &str) -> bool {
    QWEN3_PARAMETER_OPEN_REGEX
        .get_or_init(|| {
            Regex::new(QWEN3_PARAMETER_OPEN).expect("Invalid Qwen3 parameter open regex")
        })
        .is_match(params_str)
}

/// Key of a [`qwen3_parameter_regex`] capture, whichever dialect matched.
fn qwen3_parameter_key<'a>(cap: &regex::Captures<'a>) -> &'a str {
    cap.get(1)
        .or_else(|| cap.get(2))
        .or_else(|| cap.get(3))
        .map(|m| m.as_str())
        .unwrap_or_default()
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

/// Opening tag of one slot of a generic wrapper, for slot key `key` (`name`
/// or `arguments`): the `<key>` element, or a parameter opener in any
/// `QWEN3_PARAMETER_OPEN` dialect naming `key`, including the
/// separator-less `<parameterkey>` slip (0.8.4 validation, runs/review
/// turn_0042). The opener alone identifies the slot.
fn generic_wrapper_slot_open(key: &str) -> String {
    format!(
        r#"(?:<{key}>|<parameter(?:(?:\s*=\s*|\s+name\s*=\s*)(?:"{key}"|'{key}'|{key})|{key})\s*>)"#
    )
}

/// Closer of a generic-wrapper slot. Models close a slot with whichever of
/// these tags comes to mind, independent of the opener
/// (`<parameter=name>X</name>`, `<parameter=arguments>{…}</arguments>`,
/// `<name>X</parameter>`, 0.8.4 validation runs/review): the OPENER
/// identifies the slot, so any of them ends it.
const GENERIC_WRAPPER_SLOT_CLOSE: &str = r"(?:</name>|</parameter>|</arguments>)";

/// Generic wrapper whose body is exactly a name slot and an arguments slot
/// (0.8.3/0.8.4 validation, runs/long_review and runs/review):
/// `<function=tool>` + a name slot (`<name>X`, `<parameter=name>X`, any
/// parameter dialect) + an arguments slot (`<arguments>{…}` or
/// `<parameter=arguments>{…}`), each ended by any
/// `GENERIC_WRAPPER_SLOT_CLOSE`, then `</tool>` or `</function>`, and
/// nothing else in the body. Groups: 1 wrapper, 2 inner name, 3 arguments.
fn generic_wrapper_element_regex() -> &'static Regex {
    static GENERIC_WRAPPER_ELEMENT_REGEX: OnceLock<Regex> = OnceLock::new();
    GENERIC_WRAPPER_ELEMENT_REGEX.get_or_init(|| {
        Regex::new(&format!(
            r"(?s)<function=([a-zA-Z_][a-zA-Z0-9_]*)>\s*{name_open}\s*([^<]*?)\s*{close}\s*{args_open}([\s\S]*?){close}\s*(?:</tool>|</function>)",
            name_open = generic_wrapper_slot_open("name"),
            args_open = generic_wrapper_slot_open("arguments"),
            close = GENERIC_WRAPPER_SLOT_CLOSE,
        ))
        .expect("Invalid generic wrapper element regex")
    })
}

/// A [`generic_wrapper_element_regex`] match is a call only when the
/// generic-wrapper conditions of [`unwrap_generic_wrapper`] hold (a generic
/// wrapper name, an identifier-style inner name, arguments that are a JSON
/// object); anything else is left to the other families / the rejection
/// report.
fn generic_wrapper_element_call(
    cap: &regex::Captures<'_>,
    raw: String,
) -> Option<Result<ParsedToolCall>> {
    let wrapper = cap[1].trim();
    if !GENERIC_WRAPPER_NAMES.contains(&wrapper) {
        return None;
    }
    let inner_name = cap[2].trim();
    let identifier = inner_name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && inner_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !identifier {
        return None;
    }
    let arguments = serde_json::from_str::<serde_json::Value>(cap[3].trim()).ok()?;
    let call = unwrap_generic_wrapper(ParsedToolCall {
        tool_name: wrapper.to_string(),
        arguments: serde_json::json!({"name": inner_name, "arguments": arguments}),
        raw_text: raw,
        parse_method: ParseMethod::Xml,
    });
    (!GENERIC_WRAPPER_NAMES.contains(&call.tool_name.as_str())).then_some(Ok(call))
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

/// A span of the response that looked like a tool call but that no parser
/// could turn into an executable call. Surfaced to the model as a refused
/// call (never silently dropped): the model must learn the call did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseRejection {
    /// Best-effort name of the tool the model tried to call, when the region
    /// names one (`<function=x>`, `<name>x</name>`, `call tool="x"`).
    pub tool_name: Option<String>,
    /// Why the region was not executed, phrased for the model.
    pub reason: String,
    /// The rejected text (verbatim, may be long).
    pub raw_text: String,
}

/// One tool-call match of one syntax family, located in the parsed text.
struct Candidate {
    span: std::ops::Range<usize>,
    /// Family priority: lower wins when two families claim overlapping text.
    family: usize,
    result: Result<ParsedToolCall>,
}

/// Whether `pos` opens a line: only whitespace and [`TOOL_CALL_WRAPPERS`]
/// tokens precede it on its line.
fn at_line_start(content: &str, pos: usize) -> bool {
    let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    strip_wrappers(&content[line_start..pos]).is_empty()
}

/// Byte ranges (relative to `text`) of the argument VALUES a call carries as
/// raw text: Qwen3 `<parameter=k>VALUE</parameter>` (any dialect) and Kimi
/// `<|open|>argument …<|sep|>VALUE<|close|>argument`. A value ends at the
/// first closer, exactly as the family parsers read it. A value that
/// crosses another value opener (`<parameter…`, `<|open|>argument`) is not
/// one value: it ran from an unclosed example into a later call's structure,
/// so it is never returned. JSON arguments need no entry: a JSON string
/// cannot hold a raw newline, so nothing inside one is ever at line start.
fn payload_values(text: &str) -> Vec<std::ops::Range<usize>> {
    static PARAM_OPEN: OnceLock<Regex> = OnceLock::new();
    static KIMI_ARG_OPEN: OnceLock<Regex> = OnceLock::new();
    let param_open = PARAM_OPEN.get_or_init(|| {
        Regex::new(QWEN3_PARAMETER_OPEN).expect("Invalid Qwen3 parameter open regex")
    });
    let kimi_open = KIMI_ARG_OPEN.get_or_init(|| {
        Regex::new(r#"<\|open\|>argument\s+key="[^"]+"[^<]*?<\|sep\|>"#)
            .expect("Invalid Kimi argument open regex")
    });
    let mut values = Vec::new();
    for (open, close, reopen) in [
        (param_open, "</parameter>", "<parameter"),
        (kimi_open, "<|close|>argument", "<|open|>argument"),
    ] {
        for m in open.find_iter(text) {
            let Some(len) = text[m.end()..].find(close) else {
                continue;
            };
            let value = m.end()..m.end() + len;
            if !text[value.clone()].contains(reopen) {
                values.push(value);
            }
        }
    }
    values
}

/// Where a match at `span` runs into ANOTHER call: the first line-start
/// [`TOOL_CALL_OPENERS`] token inside the match that follows real payload
/// (not just the match's own opening tokens — `<tool_call>` directly
/// followed by `<function=x>` is one head). A lazy match that runs past such
/// an opener started at a prose example and swallowed the next real call.
///
/// An opener inside one of the call's own argument values (see
/// [`payload_values`]) is payload, not structure: a `file_write` of XML or
/// of docs about the tool syntax is still one call.
fn inner_call_opener(content: &str, span: &std::ops::Range<usize>) -> Option<usize> {
    static FUNCTION_TAG: OnceLock<Regex> = OnceLock::new();
    let function_tag = FUNCTION_TAG
        .get_or_init(|| Regex::new(r"<function=[^<>\s]*>?").expect("Invalid function tag regex"));
    let text = &content[span.clone()];
    let values = payload_values(text);
    let mut inner: Vec<usize> = TOOL_CALL_OPENERS
        .iter()
        .flat_map(|opener| text.match_indices(opener).map(|(p, _)| p))
        .filter(|&p| p > 0 && at_line_start(content, span.start + p))
        .filter(|&p| !in_spans(p, &values))
        .collect();
    inner.sort_unstable();
    inner.into_iter().find(|&p| {
        let mut head = function_tag.replace_all(&text[..p], "").to_string();
        for opener in TOOL_CALL_OPENERS {
            head = head.replace(opener, "");
        }
        !strip_wrappers(&head).is_empty()
    })
}

/// Every match of `regex` that can be a live call, found left to right:
/// - a match that STARTS inside markdown code (`code`: inline spans and
///   fences) is a quoted example, never a call — the scan resumes after that
///   code span, so the example cannot swallow the call that follows it;
/// - a match that runs into another call's opener (see [`inner_call_opener`])
///   is not a call either — the scan resumes AT that inner opener.
///
/// Families whose syntax IS a code fence (JSON blocks) do not use this.
fn live_matches<'c>(
    content: &'c str,
    regex: &Regex,
    code: &[std::ops::Range<usize>],
) -> Vec<regex::Captures<'c>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= content.len() {
        let Some(cap) = regex.captures_at(content, pos) else {
            break;
        };
        let whole = cap.get(0).expect("regex group 0 always matches");
        if let Some(span) = code.iter().find(|s| s.contains(&whole.start())) {
            pos = span.end.max(whole.start() + 1);
        } else if let Some(p) = inner_call_opener(content, &whole.range()) {
            pos = whole.start() + p;
        } else {
            pos = whole.end().max(whole.start() + 1);
            out.push(cap);
            continue;
        }
        while pos < content.len() && !content.is_char_boundary(pos) {
            pos += 1;
        }
    }
    out
}

/// Run one regex family over the whole text (live matches only, see
/// [`live_matches`]). `build` returns `None` for a match that is not a tool
/// call at all (e.g. a JSON block without a name).
fn regex_family(
    content: &str,
    regex: &Regex,
    family: usize,
    code: &[std::ops::Range<usize>],
    build: impl Fn(&regex::Captures<'_>, String) -> Option<Result<ParsedToolCall>>,
) -> Vec<Candidate> {
    live_matches(content, regex, code)
        .into_iter()
        .filter_map(|cap| {
            let whole = cap.get(0)?;
            let raw = whole.as_str().to_string();
            build(&cap, raw).map(|result| Candidate {
                span: whole.range(),
                family,
                result,
            })
        })
        .collect()
}

/// `<tool>`-wrapped families whose body is `name` + `<arguments>` (groups 1, 2).
fn xml_name_args_call(cap: &regex::Captures<'_>, raw: String) -> Option<Result<ParsedToolCall>> {
    let name = cap[1].trim().to_string();
    Some(
        parse_xml_arguments(cap[2].trim()).map(|arguments| ParsedToolCall {
            tool_name: name,
            arguments,
            raw_text: raw,
            parse_method: ParseMethod::Xml,
        }),
    )
}

/// Qwen3 `<function=name>` + `<parameter=…>` families (groups 1, 2).
fn qwen3_function_call(cap: &regex::Captures<'_>, raw: String) -> Option<Result<ParsedToolCall>> {
    let name = resolve_qwen_tool_name(&cap[1], &cap[2]);
    Some(
        parse_qwen3_parameters(&cap[1], &cap[2]).map(|arguments| ParsedToolCall {
            tool_name: name,
            arguments,
            raw_text: raw,
            parse_method: ParseMethod::Xml,
        }),
    )
}

fn tool_call_json_regex() -> &'static Regex {
    static TOOL_CALL_JSON_REGEX: OnceLock<Regex> = OnceLock::new();
    TOOL_CALL_JSON_REGEX.get_or_init(|| {
        Regex::new(r"(?s)<tool_call>\s*(\{.*?\})\s*</tool_call>")
            .expect("Invalid tool_call JSON regex")
    })
}

/// Every supported syntax family, matched over the WHOLE text, in priority
/// order (the order in which the families used to be tried one after the
/// other). Priority only matters when two families claim overlapping text.
fn collect_candidates(content: &str) -> Vec<Candidate> {
    let code = markdown_code_spans(content);
    let mut all = Vec::new();
    let xml_families: [&Regex; 6] = [
        xml_tool_regex(),
        xml_tool_alt_regex(),
        xml_tool_alt2_regex(),
        xml_tool_function_regex(),
        xml_tool_function_tag_regex(),
        // Qwen's missing </arguments> variant: <tool><name>x</name><arguments>{...}</tool></tool>
        xml_tool_missing_args_close_regex(),
    ];
    for (family, regex) in xml_families.into_iter().enumerate() {
        all.extend(regex_family(
            content,
            regex,
            family,
            &code,
            xml_name_args_call,
        ));
    }
    // <tool_call><function=name><parameter=key>value</parameter>...</function></tool_call>
    all.extend(regex_family(
        content,
        qwen3_tool_call_regex(),
        6,
        &code,
        qwen3_function_call,
    ));
    // <tool_call>{"name": "tool", "arguments": {...}}</tool_call> (Qwen3.5 / sglang)
    all.extend(regex_family(
        content,
        tool_call_json_regex(),
        7,
        &code,
        |cap, raw| match serde_json::from_str::<serde_json::Value>(cap[1].trim()) {
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
                Some(Ok(ParsedToolCall {
                    tool_name: name,
                    arguments,
                    raw_text: raw,
                    parse_method: ParseMethod::Json,
                }))
            }
            Err(e) => Some(Err(anyhow::anyhow!("Invalid JSON in <tool_call>: {}", e))),
        },
    ));
    // <function=tool><name>X</name><arguments>{…}</arguments></tool> (or a
    // `<parameter name="name">X</parameter>` header, or a `</function>`
    // close): the generic wrapper written with an `<arguments>` element.
    all.extend(regex_family(
        content,
        generic_wrapper_element_regex(),
        GENERIC_WRAPPER_FAMILY,
        &code,
        generic_wrapper_element_call,
    ));
    // <function=name>{"key": "value"}</function>. Outranks the bare function
    // family: both share the `<function=name>…</function>` structure, but this
    // variant carries inline JSON while the bare variant uses <parameter> tags.
    all.extend(regex_family(
        content,
        openai_function_regex(),
        9,
        &code,
        |cap, raw| {
            let name = cap[1].trim().to_string();
            Some(
                serde_json::from_str::<serde_json::Value>(cap[2].trim())
                    .map(|arguments| ParsedToolCall {
                        tool_name: name,
                        arguments,
                        raw_text: raw,
                        parse_method: ParseMethod::Xml,
                    })
                    .map_err(|e| anyhow::anyhow!("Invalid JSON in OpenAI function call: {}", e)),
            )
        },
    ));
    // <function=name><parameter=key>value</parameter>...</function>
    all.extend(regex_family(
        content,
        bare_function_regex(),
        10,
        &code,
        qwen3_function_call,
    ));
    for (family, found) in [
        (11, try_parse_kimi_tools(content, &code)),
        // Fenced JSON blocks are code by construction (not filtered by
        // `code`): `[^`]*` already stops a match at the next fence, so one
        // block cannot swallow another.
        (12, try_parse_json_blocks(content)),
        (13, try_parse_plain_function_calls(content, &code)),
    ] {
        for (result, span) in found.unwrap_or_default() {
            all.push(Candidate {
                span,
                family,
                result,
            });
        }
    }
    all
}

/// Family number of the generic-wrapper family (`generic_wrapper_element_regex`).
const GENERIC_WRAPPER_FAMILY: usize = 8;

/// Resolve overlapping candidates: every piece of text is claimed by at most
/// one call. Higher-priority families claim first (on overlap the family that
/// used to be tried first wins); the survivors are returned in text order.
///
/// Exception: a generic-wrapper candidate claims before every other family.
/// It exists only when its strict conditions hold (generic wrapper, exactly an
/// identifier name slot plus a JSON-object arguments slot), so it is always
/// the intended call; the Qwen3 `<tool_call>` family would otherwise claim the
/// same text and read `<parameter=name>X</name>…</parameter>` as one `name`
/// value, dispatching the non-tool `tool` (0.8.4 runs/review turn_0049).
///
/// First, a candidate that lies wholly inside an argument value of another
/// parsed call (see [`payload_values`]) is that call's payload — a
/// `file_write` of docs that quote a complete call — and never a call.
fn resolve_overlaps(content: &str, candidates: Vec<Candidate>) -> Vec<Candidate> {
    let values: Vec<std::ops::Range<usize>> = candidates
        .iter()
        .filter(|c| c.result.is_ok())
        .flat_map(|c| {
            let start = c.span.start;
            payload_values(&content[c.span.clone()])
                .into_iter()
                .map(move |v| start + v.start..start + v.end)
        })
        .collect();
    let mut candidates: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| {
            !values
                .iter()
                .any(|v| v.start <= c.span.start && c.span.end <= v.end)
        })
        .collect();
    candidates.sort_by_key(|c| (c.family != GENERIC_WRAPPER_FAMILY, c.family, c.span.start));
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        let overlaps = kept
            .iter()
            .any(|k| candidate.span.start < k.span.end && k.span.start < candidate.span.end);
        if !overlaps {
            kept.push(candidate);
        }
    }
    kept.sort_by_key(|c| c.span.start);
    kept
}

/// Generic wrapper names models put in the function slot while carrying the
/// real tool in a `name` parameter: `<function=tool><parameter=name>X</parameter>
/// <parameter=arguments>{…}</parameter></function>`.
const GENERIC_WRAPPER_NAMES: &[&str] = &["tool", "tool_call", "function", "call"];

/// Unwrap a generic-wrapper call to the real tool: when the tool name is one
/// of [`GENERIC_WRAPPER_NAMES`] and the arguments are exactly `name` (a
/// string) plus `arguments` (a JSON object, or a string holding one).
fn unwrap_generic_wrapper(mut call: ParsedToolCall) -> ParsedToolCall {
    if !GENERIC_WRAPPER_NAMES.contains(&call.tool_name.trim()) {
        return call;
    }
    let Some(obj) = call.arguments.as_object() else {
        return call;
    };
    if obj.len() != 2 {
        return call;
    }
    let Some(inner_name) = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
    else {
        return call;
    };
    let inner_args = match obj.get("arguments") {
        Some(v @ serde_json::Value::Object(_)) => v.clone(),
        Some(serde_json::Value::String(s)) => {
            match serde_json::from_str::<serde_json::Value>(s.trim()) {
                Ok(v @ serde_json::Value::Object(_)) => v,
                _ => return call,
            }
        }
        _ => return call,
    };
    call.tool_name = inner_name.to_string();
    call.arguments = inner_args;
    call
}

/// Byte ranges of markdown code: fenced blocks (```/~~~, to the closing fence
/// or end of text) and inline backtick spans. Text inside is quoted, never a
/// live tool call or reasoning marker.
pub(crate) fn markdown_code_spans(content: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans = Vec::new();
    let bytes = content.as_bytes();
    let mut line_start = 0usize;
    let mut fence: Option<(usize, &str)> = None; // (start, marker)
    while line_start < content.len() {
        let line_end = content[line_start..]
            .find('\n')
            .map(|i| line_start + i + 1)
            .unwrap_or(content.len());
        let line = &content[line_start..line_end];
        let trimmed = line.trim_start();
        match fence {
            Some((start, marker)) => {
                if trimmed.starts_with(marker) {
                    spans.push(start..line_end);
                    fence = None;
                }
            }
            None => {
                if trimmed.starts_with("```") {
                    fence = Some((line_start, "```"));
                } else if trimmed.starts_with("~~~") {
                    fence = Some((line_start, "~~~"));
                } else {
                    // Inline code spans on this line: a run of N backticks
                    // closed by the next run of exactly N backticks.
                    let mut i = line_start;
                    while i < line_end {
                        if bytes[i] != b'`' {
                            i += 1;
                            continue;
                        }
                        let open_start = i;
                        while i < line_end && bytes[i] == b'`' {
                            i += 1;
                        }
                        let run = i - open_start;
                        let mut j = i;
                        let mut closed = None;
                        while j < line_end {
                            if bytes[j] == b'`' {
                                let close_start = j;
                                while j < line_end && bytes[j] == b'`' {
                                    j += 1;
                                }
                                if j - close_start == run {
                                    closed = Some(j);
                                    break;
                                }
                            } else {
                                j += 1;
                            }
                        }
                        if let Some(end) = closed {
                            spans.push(open_start..end);
                            i = end;
                        }
                    }
                }
            }
        }
        line_start = line_end;
    }
    if let Some((start, _)) = fence {
        spans.push(start..content.len());
    }
    spans
}

/// `content` with every markdown code span (see [`markdown_code_spans`])
/// replaced by one space: the prose a tool-markup check may look at. Quoted
/// syntax is never a call attempt.
pub(crate) fn outside_markdown_code(content: &str) -> String {
    let mut prose = String::with_capacity(content.len());
    let mut cursor = 0;
    for span in markdown_code_spans(content) {
        prose.push_str(&content[cursor..span.start]);
        prose.push(' ');
        cursor = span.end;
    }
    prose.push_str(&content[cursor..]);
    prose
}

fn in_spans(pos: usize, spans: &[std::ops::Range<usize>]) -> bool {
    spans.iter().any(|s| s.contains(&pos))
}

/// Openers of a tool-call region in any supported syntax.
const TOOL_CALL_OPENERS: &[&str] = &["<tool_call>", "<tool>", "<function=", "<|open|>call "];

/// Wrapper tokens that carry no call on their own (a stray `</tool_call>`
/// after a parsed call, a `<tool_call>` directly around a parsed call).
const TOOL_CALL_WRAPPERS: &[&str] = &[
    "<tool_call>",
    "</tool_call>",
    "</tool>",
    "</function>",
    "<|open|>tools<|sep|>",
    "<|close|>tools<|sep|>",
    "<|close|>tools",
];

fn strip_wrappers(text: &str) -> String {
    let mut out = text.to_string();
    for w in TOOL_CALL_WRAPPERS {
        out = out.replace(w, "");
    }
    out.trim().to_string()
}

/// Best-effort name of the tool a rejected region tried to call.
fn guess_rejected_tool_name(region: &str) -> Option<String> {
    static NAME_REGEX: OnceLock<Regex> = OnceLock::new();
    let re = NAME_REGEX.get_or_init(|| {
        Regex::new(
            r#"<function=([A-Za-z_][A-Za-z0-9_]*)|<name>\s*([A-Za-z_][A-Za-z0-9_]*)\s*</name>|<parameter=name>\s*([A-Za-z_][A-Za-z0-9_]*)\s*</parameter>|call\s+tool="([^"]+)""#,
        )
        .expect("Invalid rejected-name regex")
    });
    let mut generic = None;
    for cap in re.captures_iter(region) {
        let name = (1..=4)
            .find_map(|i| cap.get(i))
            .map(|m| m.as_str().to_string())?;
        if GENERIC_WRAPPER_NAMES.contains(&name.as_str()) {
            generic.get_or_insert(name);
        } else {
            return Some(name);
        }
    }
    generic
}

const REJECTION_FORMAT_HINT: &str =
    "Re-issue it as <tool><name>TOOL_NAME</name><arguments>{JSON object}</arguments></tool>.";

fn rejection_preview(raw: &str) -> String {
    const MAX: usize = 200;
    let raw = raw.trim();
    if raw.len() <= MAX {
        raw.to_string()
    } else {
        format!("{}…", &raw[..raw.floor_char_boundary(MAX)])
    }
}

/// Regions outside every claimed span that open like a tool call (a
/// [`TOOL_CALL_OPENERS`] token at line start, outside markdown code) but that
/// no parser accepted. Prose and quoted code never produce a rejection.
fn find_unparsed_regions(
    content: &str,
    claimed: &[std::ops::Range<usize>],
) -> Vec<std::ops::Range<usize>> {
    let code = markdown_code_spans(content);
    let mut openers: Vec<usize> = Vec::new();
    for opener in TOOL_CALL_OPENERS {
        for (pos, _) in content.match_indices(opener) {
            if in_spans(pos, claimed) || in_spans(pos, &code) {
                continue;
            }
            let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
            // Only wrapper tokens (or nothing) may precede the opener on its line.
            if !strip_wrappers(&content[line_start..pos]).is_empty() {
                continue;
            }
            openers.push(pos);
        }
    }
    openers.sort_unstable();
    openers.dedup();

    let mut regions: Vec<std::ops::Range<usize>> = Vec::new();
    for &pos in &openers {
        if let Some(last) = regions.last_mut() {
            // `<tool_call>` directly followed by `<function=…>` is ONE call.
            if pos <= last.end || strip_wrappers(&content[last.start..pos]).is_empty() {
                continue;
            }
        }
        let next_claim = claimed
            .iter()
            .map(|s| s.start)
            .filter(|&s| s > pos)
            .min()
            .unwrap_or(content.len());
        regions.push(pos..next_claim);
    }
    // Split regions at later openers that start a new call, and keep only
    // regions that carry more than wrapper tokens.
    let mut split: Vec<std::ops::Range<usize>> = Vec::new();
    for region in regions {
        let mut start = region.start;
        let inner: Vec<usize> = openers
            .iter()
            .copied()
            .filter(|&p| p > region.start && p < region.end)
            .collect();
        for p in inner {
            if !strip_wrappers(&content[start..p]).is_empty()
                && (content[p..].starts_with("<tool_call>") || content[p..].starts_with("<tool>"))
            {
                split.push(start..p);
                start = p;
            }
        }
        split.push(start..region.end);
    }
    split
        .into_iter()
        .filter(|r| !strip_wrappers(&content[r.clone()]).is_empty())
        .collect()
}

/// Parse content for tool calls using multiple strategies.
///
/// Every supported syntax family is matched over the whole text, overlapping
/// matches resolve to one call (two parsers never both claim the same text),
/// and the calls come back in text order — so a response that mixes syntaxes
/// yields every call, not only the first family's. The same call written in
/// two syntaxes executes once. Text that opens like a tool call but that no
/// parser accepts is reported in [`ParseResult::rejections`].
pub fn parse_tool_calls(content: &str) -> ParseResult {
    // Enforce maximum input size to prevent pathological regex performance.
    // Use char-boundary-safe truncation to avoid panics on multi-byte UTF-8.
    let content = if content.len() > MAX_TOOL_PARSER_INPUT_SIZE {
        &content[..content.floor_char_boundary(MAX_TOOL_PARSER_INPUT_SIZE)]
    } else {
        content
    };

    // Fix malformed closing tags (e.g., `arguments>` → `</arguments>`)
    // before running the regex parsers.
    let content = normalize_malformed_xml(content);
    let content = content.as_str();

    let mut result = ParseResult {
        tool_calls: Vec::new(),
        text_content: String::new(),
        parse_errors: Vec::new(),
        rejections: Vec::new(),
    };

    // Warn about unclosed tool tags (common with Qwen3.5 quantized models)
    if content.contains("<tool>") && !content.contains("</tool>") {
        // floor_char_boundary: byte-slicing at 300 panics when a multi-byte
        // char straddles the cut (exactly the flaky-model case this warns on).
        tracing::warn!(
            "Unclosed <tool> tag detected — tool call may be lost. Content preview: {}",
            &content[..content.floor_char_boundary(300)]
        );
    }

    let kept = resolve_overlaps(content, collect_candidates(content));
    let claimed: Vec<std::ops::Range<usize>> = kept.iter().map(|c| c.span.clone()).collect();

    // Parser errors (e.g. invalid JSON inside <tool_call>) and regions no
    // parser accepted, in text order.
    let mut rejected: Vec<(usize, ParseRejection)> = Vec::new();
    // Spans removed from the text content: the executed calls, and restated
    // duplicates of them.
    let mut removed: Vec<std::ops::Range<usize>> = Vec::new();
    let mut seen: Vec<(usize, String, serde_json::Value)> = Vec::new();
    for candidate in kept {
        match candidate.result {
            Ok(call) => {
                let call = unwrap_generic_wrapper(call);
                removed.push(candidate.span.clone());
                // The same call restated in another syntax runs once.
                let duplicate = seen.iter().any(|(family, name, args)| {
                    *family != candidate.family
                        && *name == call.tool_name
                        && *args == call.arguments
                });
                if duplicate {
                    tracing::debug!(
                        "Dropping restated duplicate of '{}' written in a second syntax",
                        call.tool_name
                    );
                    continue;
                }
                seen.push((
                    candidate.family,
                    call.tool_name.clone(),
                    call.arguments.clone(),
                ));
                result.tool_calls.push(call);
            }
            Err(e) => {
                let raw = &content[candidate.span.clone()];
                result
                    .parse_errors
                    .push(format!("Tool call parse error: {}", e));
                rejected.push((
                    candidate.span.start,
                    ParseRejection {
                        tool_name: guess_rejected_tool_name(raw),
                        reason: format!(
                            "Tool call NOT executed: it could not be parsed ({}). {} Call text: {}",
                            e,
                            REJECTION_FORMAT_HINT,
                            rejection_preview(raw)
                        ),
                        raw_text: raw.to_string(),
                    },
                ));
            }
        }
    }
    for region in find_unparsed_regions(content, &claimed) {
        let raw = &content[region.clone()];
        let tool_name = guess_rejected_tool_name(raw);
        let what = tool_name
            .as_deref()
            .map(|n| format!("a call to '{}'", n))
            .unwrap_or_else(|| "a tool call".to_string());
        result.parse_errors.push(format!(
            "Unparsed tool-call markup: {}",
            rejection_preview(raw)
        ));
        rejected.push((
            region.start,
            ParseRejection {
                tool_name,
                reason: format!(
                    "Tool call NOT executed: {} was written in a malformed or mixed syntax that no parser accepts. {} Call text: {}",
                    what,
                    REJECTION_FORMAT_HINT,
                    rejection_preview(raw)
                ),
                raw_text: raw.to_string(),
            },
        ));
    }
    rejected.sort_by_key(|(start, _)| *start);
    result.rejections = rejected.into_iter().map(|(_, r)| r).collect();

    // Text content: everything outside the executed calls, minus the Kimi
    // container tokens that wrap them.
    removed.sort_by_key(|r| r.start);
    let mut text = String::with_capacity(content.len());
    let mut cursor = 0;
    for span in removed {
        if span.start > cursor {
            text.push_str(&content[cursor..span.start]);
        }
        cursor = cursor.max(span.end);
    }
    text.push_str(&content[cursor.min(content.len())..]);
    if content.contains("call tool=") {
        for token in [
            "<|open|>tools<|sep|>",
            "<|close|>tools<|sep|>",
            "<|close|>tools",
            "<|close|>message<|sep|>",
            "<|close|>message",
        ] {
            text = text.replace(token, "");
        }
    }
    result.text_content = text.trim().to_string();

    result
}

/// A parsed (or failed) call with the byte span of the text it came from.
type SpannedCall = (Result<ParsedToolCall>, std::ops::Range<usize>);

/// Try to parse Moonshot/Kimi delimiter-style tool calls:
/// `<|open|>call tool="name" index="1"<|sep|><|open|>argument key="arg" type="string"<|sep|>val<|close|>argument<|close|>call`
fn try_parse_kimi_tools(
    content: &str,
    code: &[std::ops::Range<usize>],
) -> Option<Vec<SpannedCall>> {
    if !content.contains("call tool=") {
        return None;
    }
    let regex = kimi_call_regex();
    let arg_regex = kimi_arg_regex();

    let results: Vec<_> = live_matches(content, regex, code)
        .into_iter()
        .map(|cap| {
            let whole = cap.get(0).expect("regex group 0 always matches");
            let raw = whole.as_str().to_string();
            let span = whole.range();
            let tool_name = cap[1].trim().to_string();
            let body = &cap[2];

            let mut args_map = serde_json::Map::new();
            let mut found_any = false;

            for arg_cap in arg_regex.captures_iter(body) {
                found_any = true;
                let key = arg_cap[1].trim().to_string();
                let type_hint = arg_cap.get(2).map(|m| m.as_str());
                let val_str = arg_cap[3].trim();

                let json_val = if let Some("number") = type_hint {
                    if let Ok(i) = val_str.parse::<i64>() {
                        serde_json::Value::Number(i.into())
                    } else if let Ok(f) = val_str.parse::<f64>() {
                        serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or_else(|| serde_json::Value::String(val_str.to_string()))
                    } else {
                        serde_json::Value::String(val_str.to_string())
                    }
                } else if let Some("boolean") = type_hint {
                    match val_str.to_lowercase().as_str() {
                        "true" => serde_json::Value::Bool(true),
                        "false" => serde_json::Value::Bool(false),
                        _ => serde_json::Value::String(val_str.to_string()),
                    }
                } else if (val_str.starts_with('{') && val_str.ends_with('}'))
                    || (val_str.starts_with('[') && val_str.ends_with(']'))
                {
                    serde_json::from_str::<serde_json::Value>(val_str)
                        .unwrap_or_else(|_| serde_json::Value::String(val_str.to_string()))
                } else {
                    serde_json::Value::String(val_str.to_string())
                };

                args_map.insert(key, json_val);
            }

            let arguments = if found_any {
                serde_json::Value::Object(args_map)
            } else if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(body.trim()) {
                parsed_json
            } else {
                serde_json::json!({})
            };

            let call = ParsedToolCall {
                tool_name,
                arguments,
                raw_text: raw.clone(),
                parse_method: ParseMethod::Xml,
            };

            (Ok(call), span)
        })
        .collect();

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Top-level `<name>` / `<arguments>` blocks of a Qwen hybrid call body.
///
/// Only blocks that form the *leading structure* of the body count: the body is
/// scanned from the start, skipping whitespace, and each step must open with
/// `<name>` or `<arguments>` (each at most once). The scan stops at the first
/// other token, so `<name>`/`<arguments>` text that appears later — inside a
/// `<parameter=content>` value, or in free text — is payload, never structure.
#[derive(Default)]
struct QwenHybridHeader<'a> {
    name: Option<&'a str>,
    arguments: Option<&'a str>,
}

fn qwen_hybrid_header(params_str: &str) -> QwenHybridHeader<'_> {
    let mut header = QwenHybridHeader::default();
    let mut rest = params_str;
    loop {
        rest = rest.trim_start();
        let (slot, tag) = if rest.starts_with("<name>") && header.name.is_none() {
            (&mut header.name, "name")
        } else if rest.starts_with("<arguments>") && header.arguments.is_none() {
            (&mut header.arguments, "arguments")
        } else {
            break;
        };
        let open_len = tag.len() + 2;
        let close = format!("</{}>", tag);
        let after = &rest[open_len..];
        let Some(end) = after.find(&close) else {
            break;
        };
        *slot = Some(&after[..end]);
        rest = &after[end + close.len()..];
    }
    header
}

/// Whether the hybrid `<function=tool><name>x</name><arguments>{…}</arguments>`
/// form may apply: only when the outer function name is the generic `tool`, or
/// when the body carries no `<parameter=` tags at all. A real tool call such as
/// `<function=file_write><parameter=content>…` must never have its arguments
/// replaced by `<arguments>` text appearing inside a parameter value.
fn qwen_hybrid_form_allowed(captured_name: &str, params_str: &str) -> bool {
    captured_name.trim() == "tool" || !has_qwen3_parameter_tag(params_str)
}

/// Resolve the actual tool name for Qwen tool calls. If the outer function name
/// is generic (`"tool"`), extract the real tool name from the leading top-level
/// `<name>` tag (never from inside a `<parameter=…>` value or argument payload).
fn resolve_qwen_tool_name(captured_name: &str, params_str: &str) -> String {
    let trimmed = captured_name.trim();
    if trimmed == "tool" {
        if let Some(inner) = qwen_hybrid_header(params_str).name {
            let inner = inner.trim();
            if !inner.is_empty() {
                return inner.to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Parse Qwen3-style parameters: <parameter=key>value</parameter>, or the hybrid
/// leading `<arguments>...</arguments>` form (see [`qwen_hybrid_form_allowed`]
/// and [`qwen_hybrid_header`]).
fn parse_qwen3_parameters(captured_name: &str, params_str: &str) -> Result<serde_json::Value> {
    if qwen_hybrid_form_allowed(captured_name, params_str) {
        if let Some(inner) = qwen_hybrid_header(params_str).arguments {
            return parse_xml_arguments(inner);
        }
    }
    let param_regex = qwen3_parameter_regex();
    let mut args = serde_json::Map::new();

    for cap in param_regex.captures_iter(params_str) {
        let key = qwen3_parameter_key(&cap).to_string();
        let raw_value = cap[4].trim();
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
fn try_parse_json_blocks(content: &str) -> Option<Vec<SpannedCall>> {
    let regex = json_block_regex();

    if !regex.is_match(content) {
        return None;
    }

    let results: Vec<_> = regex
        .captures_iter(content)
        .filter_map(|cap| {
            let whole = cap.get(0).expect("regex group 0 always matches");
            let raw = whole.as_str().to_string();
            let span = whole.range();
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
                            span,
                        ))
                    } else {
                        None
                    }
                }
                Err(e) => Some((Err(anyhow::anyhow!("Invalid JSON: {}", e)), span)),
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
fn try_parse_plain_function_calls(
    content: &str,
    code: &[std::ops::Range<usize>],
) -> Option<Vec<SpannedCall>> {
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
        let whole = cap.get(0).expect("regex group 0 always matches");
        // A call line inside markdown code is a quoted example.
        if in_spans(whole.start(), code) {
            continue;
        }
        let raw = whole.as_str().to_string();
        let span = whole.range();
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
            span,
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
