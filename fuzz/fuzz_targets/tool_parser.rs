#![no_main]
use libfuzzer_sys::fuzz_target;
use selfware::tool_parser;

fuzz_target!(|data: &[u8]| {
    if let Ok(content_str) = std::str::from_utf8(data) {
        let _ = tool_parser::parse_tool_calls(content_str);
        let _ = tool_parser::extract_text_only(content_str);
    }
});