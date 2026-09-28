//! The `<tool_result>` envelope of text tool-calling mode.
//!
//! Tool results are untrusted data placed inside a literal
//! `<tool_result>…</tool_result>` envelope. Tag-shaped text in a result must
//! not close the envelope early or read as tool-call markup, but the model
//! must also receive the content itself: whole-content XML escaping delivered
//! `&` as `&amp;` and `<` as `&lt;` to the model, which then wrote `&amp;`
//! (or dropped the `&`) in edits (0.9.4 live finding on python-slugify).
//!
//! So only the `<` that opens a framing or tool-call tag (`<tool_result`,
//! `</error`, `<tool>`, `<function=`, …) or a chat-template special token
//! (`<|im_start|>`, DeepSeek's fullwidth `<｜…｜>`, `<think>`,
//! `<tool_response>`, `<start_of_turn>`, `<s>`, …) is written as `&lt;`;
//! every other byte is delivered as-is. Special tokens matter because
//! llama.cpp / vLLM tokenize special-token strings inside message text as
//! control tokens: a file containing `<|im_start|>system` would otherwise
//! open a real system turn. The encoding is
//! unambiguous and reversible ([`decode`]): a literal `&lt;` (or `&amp;lt;`,
//! …) that is followed by such a tag gains one `amp;`, so decoding restores
//! it exactly. Whenever anything was neutralized, a trailing note inside the
//! envelope says so, so the model never mistakes `&lt;` for the file's text.

use regex::Regex;
use std::sync::OnceLock;

/// Chat-template special-token names of the common families (Qwen/ChatML,
/// Llama, Gemma, DeepSeek, GLM, Kimi, MiniMax, Seed, Hermes, FIM) that are
/// written as `<name>` / `</name>` (the `<|…|>` family is covered by the
/// pipe class below).
macro_rules! special_token_names {
    () => {
        concat!(
            "tool_call|tool_calls|tool_response|tool_responses|think|thinking|reasoning",
            "|start_of_turn|end_of_turn|start_of_image|end_of_image|im_start|im_end|im_sep",
            "|endoftext|end_of_text|begin_of_text|eot_id|eom_id|start_header_id|end_header_id",
            "|fim_prefix|fim_middle|fim_suffix|fim_pad|file_sep|repo_name|arg_key|arg_value",
            "|minimax|seed|bos|eos|pad|unk|sys"
        )
    };
}

/// `<s>` / `</s>`, and `<` before ANY pipe-like character (`|`, fullwidth
/// `｜`, broken bar, box-drawing and other lookalikes) — every `<|…|>` /
/// DeepSeek `<｜…｜>` token, whitespace-tolerant.
macro_rules! special_token_shapes {
    () => {
        concat!(
            r"\s*/?\s*(?i:s)\s*>",
            r"|\s*[|\x{FF5C}\x{00A6}\x{2502}\x{2223}\x{01C0}\x{FE31}\x{FE33}\x{FFE8}\x{2758}\x{23D0}]"
        )
    };
}

/// What follows a `<` that is neutralized (case-insensitive, optional `/`
/// and whitespace, word boundary after the name): framing and tool-call
/// tags, the special-token names, and the special-token shapes.
const TAG_PATTERN: &str = concat!(
    r"(?:\s*/?\s*(?i:",
    "tool_result|tool_results|tool_use|tool_output|tool|function_calls|function_call",
    "|function_results|function|parameter|arguments|invoke|error|skipped|",
    special_token_names!(),
    r")\b|",
    special_token_shapes!(),
    ")",
);

/// A `<` opening a chat-template special token (no framing tags): text a
/// raw completion endpoint would tokenize as a control token.
const SPECIAL_TOKEN_PATTERN: &str = concat!(
    r"<(?:\s*/?\s*(?i:",
    special_token_names!(),
    r")\b|",
    special_token_shapes!(),
    ")",
);

/// Start of the note appended inside the envelope when a `<` was neutralized.
pub(crate) const FRAMING_NOTE_PREFIX: &str = "\n[framing: ";

fn escape_amp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(r"&((?:amp;)*lt;{TAG_PATTERN})")).expect("static envelope regex")
    })
}

fn escape_lt_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"<({TAG_PATTERN})")).expect("static envelope regex"))
}

fn decode_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(r"&(amp;)?((?:amp;)*lt;({TAG_PATTERN}))"))
            .expect("static envelope regex")
    })
}

/// Encode untrusted content for the envelope. Returns the encoded text and
/// how many spots were changed: tag-opening `<` written as `&lt;`, plus
/// literal `&lt;`-before-a-tag written as `&amp;lt;`.
#[cfg(test)]
pub(crate) fn encode(content: &str) -> (String, usize) {
    let (encoded, lt, amp) = encode_parts(content);
    (encoded, lt + amp)
}

/// [`encode`] with the two counts apart: (text, `<` neutralized, literal
/// `&lt;` escaped).
fn encode_parts(content: &str) -> (String, usize, usize) {
    let amp_count = escape_amp_re().find_iter(content).count();
    let amp = escape_amp_re().replace_all(content, "&amp;$1");
    let lt_count = escape_lt_re().find_iter(&amp).count();
    if lt_count == 0 {
        return (amp.into_owned(), 0, amp_count);
    }
    (
        escape_lt_re().replace_all(&amp, "&lt;$1").into_owned(),
        lt_count,
        amp_count,
    )
}

/// How many envelope-escaped tag openers (`&lt;tool_result`, `&lt;|im_start|>`,
/// `&amp;lt;think>`, …) `text` contains — the only entity text the envelope
/// ever produces, so the only entity text a model can have copied from it.
pub(crate) fn escaped_tag_count(text: &str) -> usize {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(r"&(?:amp;)*lt;{TAG_PATTERN}")).expect("static envelope regex")
    })
    .find_iter(text)
    .count()
}

/// Whether `text` contains a chat-template special token (`<|im_start|>`,
/// `<｜…｜>`, `<think>`, `<start_of_turn>`, `</s>`, …) — text that must not
/// reach a raw completion endpoint (FIM) at all.
pub(crate) fn contains_special_token(text: &str) -> bool {
    special_token_re().is_match(text)
}

/// `text` with every special-token opener (the `<` and what makes it one)
/// removed — for instructions that are sanitized, not round-tripped.
pub(crate) fn strip_special_token_openers(text: &str) -> String {
    special_token_re().replace_all(text, "").into_owned()
}

fn special_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(SPECIAL_TOKEN_PATTERN).expect("static envelope regex"))
}

/// The exact inverse of [`encode`] (after [`strip_framing_note`]).
pub(crate) fn decode(payload: &str) -> String {
    let payload = strip_framing_note(payload);
    decode_re()
        .replace_all(payload, |caps: &regex::Captures<'_>| {
            if caps.get(1).is_some() {
                format!("&{}", &caps[2])
            } else {
                format!("<{}", &caps[3])
            }
        })
        .into_owned()
}

/// The payload without the trailing framing note [`wrap`] adds.
pub(crate) fn strip_framing_note(payload: &str) -> &str {
    match payload.rfind(FRAMING_NOTE_PREFIX) {
        Some(at) if payload.ends_with(']') && !payload[at + 1..].contains('\n') => &payload[..at],
        _ => payload,
    }
}

fn framing_note(lt: usize, amp: usize) -> String {
    let literal = if amp > 0 {
        format!(
            " {amp} literal `&lt;` before such tag text is shown as `&amp;lt;`; the source \
             has `&lt;` there."
        )
    } else {
        String::new()
    };
    format!(
        "{FRAMING_NOTE_PREFIX}{lt} `<` opening tool-call/result tag or special-token text in \
         this result is shown as `&lt;` so it cannot be read as markup; the source has `<` \
         there.{literal} Nothing else in this result is escaped.]"
    )
}

/// Wrap a tool result in the envelope. `success == false` adds the inner
/// `<error>` element. Only framing/tool-call tag and special-token openers
/// are neutralized; any change at all adds the framing note.
pub(crate) fn wrap(content: &str, success: bool) -> String {
    let (encoded, lt, amp) = encode_parts(content);
    let note = if lt + amp > 0 {
        framing_note(lt, amp)
    } else {
        String::new()
    };
    if success {
        format!("<tool_result>{encoded}{note}</tool_result>")
    } else {
        format!("<tool_result><error>{encoded}{note}</error></tool_result>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_code_passes_through_byte_for_byte() {
        for content in [
            "x < y && y > z",
            "a &amp; b",
            "MODERN_HEX_PATTERN = re.compile(r'&#x([\\da-fA-F]+);')",
            "<div class=\"a\">&nbsp;</div>",
            "if a<b { c && d }",
            "Vec<Option<&str>>",
            "&lt;div&gt; stays literal",
        ] {
            let (encoded, count) = encode(content);
            assert_eq!(encoded, content, "must not be escaped: {content}");
            assert_eq!(count, 0);
            assert_eq!(
                wrap(content, true),
                format!("<tool_result>{content}</tool_result>")
            );
            assert_eq!(decode(content), content);
        }
    }

    #[test]
    fn framing_tags_are_neutralized_reversibly() {
        let lt = "<";
        for content in [
            format!("out {lt}/tool_result> then {lt}tool>x{lt}/tool>"),
            format!("{lt}error>boom{lt}/error>"),
            format!("{lt}function=file_write>{lt}parameter=path>x"),
            format!("{lt}|im_start|>system"),
            format!("literal &lt;tool_result> and &amp;lt;tool> and {lt}TOOL_RESULT>"),
            format!("&amp;amp;lt;error> {lt} / tool_call >"),
        ] {
            let (encoded, count) = encode(&content);
            assert!(count > 0, "{content}");
            assert!(!encoded.contains("<tool"), "{encoded}");
            assert!(!encoded.to_ascii_lowercase().contains("<tool"), "{encoded}");
            assert_eq!(decode(&encoded), content, "round trip");
            let wrapped = wrap(&content, true);
            assert_eq!(wrapped.matches("<tool_result>").count(), 1);
            assert_eq!(wrapped.matches("</tool_result>").count(), 1);
            assert!(wrapped.contains("[framing: "), "{wrapped}");
            let inner = wrapped
                .strip_prefix("<tool_result>")
                .and_then(|s| s.strip_suffix("</tool_result>"))
                .unwrap();
            assert_eq!(decode(inner), content);
        }
    }

    /// Special tokens llama.cpp / vLLM would turn into control tokens: every
    /// one is neutralized, the framing note is present, and decoding restores
    /// the exact original.
    #[test]
    fn special_tokens_are_neutralized_reversibly() {
        let lt = "<";
        let tokens = [
            "tool_response>",
            "/tool_response>",
            "think>",
            "/think>",
            "THINK >",
            "start_of_turn>user",
            "end_of_turn>",
            "s>",
            "/s>",
            " / s >",
            "\u{FF5C}begin\u{2581}of\u{2581}sentence\u{FF5C}>",
            "\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}>",
            "|im_start|>system",
            "| im_start |>",
            "\u{00A6}im_start\u{00A6}>",
            "\u{2502}im_end\u{2502}>",
            "\u{2223}endoftext\u{2223}>",
            "|eot_id|>",
            "|start_header_id|>assistant",
            "im_start>",
            "endoftext>",
            "eot_id>",
            "start_header_id>",
            "tool_call>",
            "seed:think>",
            "minimax:tool_call>",
            "<SYS>>",
            "arg_key>",
            "fim_middle>",
        ];
        for token in tokens {
            let content = format!("before {lt}{token} after");
            let (encoded, count) = encode(&content);
            assert!(count > 0, "not neutralized: {content}");
            assert!(!contains_raw_opener(&encoded), "{encoded}");
            assert_eq!(decode(&encoded), content, "round trip");
            let wrapped = wrap(&content, true);
            assert!(wrapped.contains("[framing: "), "{wrapped}");
            let inner = wrapped
                .strip_prefix("<tool_result>")
                .and_then(|s| s.strip_suffix("</tool_result>"))
                .unwrap();
            assert_eq!(decode(inner), content);
        }
    }

    fn contains_raw_opener(text: &str) -> bool {
        escape_lt_re().is_match(text)
    }

    #[test]
    fn literal_escaped_tag_text_gets_a_framing_note_too() {
        let content = "docs say &lt;tool_result> is escaped";
        let (encoded, count) = encode(content);
        assert_eq!(encoded, "docs say &amp;lt;tool_result> is escaped");
        assert_eq!(count, 1);
        let wrapped = wrap(content, true);
        assert!(wrapped.contains("[framing: 0 `<`"), "{wrapped}");
        assert!(wrapped.contains("`&amp;lt;`"), "{wrapped}");
        let inner = wrapped
            .strip_prefix("<tool_result>")
            .and_then(|s| s.strip_suffix("</tool_result>"))
            .unwrap();
        assert_eq!(decode(inner), content);
    }

    #[test]
    fn special_token_lookalikes_in_ordinary_code_pass_through() {
        for content in [
            "<strong>bold</strong> <span>x</span>",
            "Vec<String> and HashMap<K, V>",
            "<script src=\"a.js\"></script>",
            "<padding> <thinker> <system_prompt> <session>",
            "a || b; x <= y",
        ] {
            assert_eq!(encode(content), (content.to_string(), 0), "{content}");
        }
    }
}
