use super::*;
use crate::agent::context::{ContextCompressor, WorkLedger};
use crate::agent::Agent;
use crate::token_count::estimate_content_tokens;

// ---------------------------------------------------------------------------
// Fixtures built from the val082 turn-artifact shapes
// ---------------------------------------------------------------------------
//
// c24 (context 24,000, max_tokens 8,192 -> 11,008-token history budget,
// threshold 8,256): system prompt ~21k chars (~5.3k tokens), task ~2.4k
// chars, whole-file reads of ~21k chars (~5.5k tokens), text tool calling
// (`<tool_result>` user messages), 2–5 messages per request.
//
// b2_65536 (65,536 / 8,192 -> 44,237-token budget, threshold 33,177): the
// same system prompt, reads of 20–41k chars (5–12k tokens) and one of
// verification.rs at 136k chars (~35k tokens).

/// A Rust source file of roughly `target_tokens` tokens (measured): a
/// `pub fn` every 12 lines, `#[test]` fns in the second half.
fn rust_source(stem: &str, target_tokens: usize) -> String {
    let mut out = String::from("use std::collections::HashMap;\n\n");
    let mut i = 0usize;
    while estimate_content_tokens(&out) < target_tokens {
        out.push_str(&format!(
            "/// Doc for {stem}_{i}.\npub fn {stem}_{i}(input: &str) -> usize {{\n    let mut \
             total = 0usize;\n    for (n, ch) in input.chars().enumerate() {{\n        if \
             ch.is_alphanumeric() {{\n            total += n % 7;\n        }}\n    }}\n    \
             total + {i}\n}}\n\n"
        ));
        i += 1;
    }
    out
}

fn native_call(id: &str, name: &str, args: serde_json::Value) -> Message {
    let mut message = Message::assistant("");
    message.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: id.to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }]);
    message
}

fn read_payload(content: &str) -> String {
    serde_json::json!({
        "content": content,
        "encoding": "utf-8",
        "total_lines": content.lines().count(),
        "truncated": false,
        "valid_utf8": true,
    })
    .to_string()
}

fn xml_call(path: &str) -> Message {
    Message::assistant(format!(
        "<tool>\n<name>file_read</name>\n<arguments>{{\"path\": \"{path}\"}}</arguments>\n</tool>"
    ))
}

fn xml_result(payload: &str) -> Message {
    let escaped = payload
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    Message::user(format!("<tool_result>{escaped}</tool_result>"))
}

const TASK: &str = "TASK SENTINEL: review src/agent/*.rs read-only and report with path:line \
                    citations for every finding.";

fn system_prompt() -> Message {
    // ~5.3k tokens like the live system prompt.
    Message::system(
        "You are selfware, a careful coding agent. Follow the tool protocol.\n".repeat(380),
    )
}

/// b2_65536 shape, native tool calling: system, task, then `reads` whole-file
/// reads of ~`read_tokens` each.
fn native_history(reads: &[(&str, usize)]) -> Vec<Message> {
    let mut messages = vec![system_prompt(), Message::user(TASK)];
    for (i, (path, tokens)) in reads.iter().enumerate() {
        let id = format!("call_{i}");
        let stem = format!("f{i}");
        messages.push(native_call(
            &id,
            "file_read",
            serde_json::json!({ "path": path }),
        ));
        messages.push(Message::tool(
            read_payload(&rust_source(&stem, *tokens)),
            id,
        ));
    }
    messages
}

fn assert_pairs_valid(messages: &[Message]) {
    for (i, m) in messages.iter().enumerate() {
        if let Some(calls) = m.tool_calls.as_deref() {
            for call in calls {
                assert!(
                    messages[i + 1..]
                        .iter()
                        .any(|r| r.role == "tool"
                            && r.tool_call_id.as_deref() == Some(call.id.as_str())),
                    "call {} lost its result",
                    call.id
                );
            }
        }
        if m.role == "tool" {
            let id = m.tool_call_id.as_deref().unwrap();
            assert!(
                messages[..i].iter().any(|a| a
                    .tool_calls
                    .as_ref()
                    .is_some_and(|c| c.iter().any(|c| c.id == id))),
                "result {id} lost its call"
            );
        }
    }
}

fn payload_of(m: &Message) -> serde_json::Value {
    let text = m.content.text();
    let inner = if let Some(start) = text.find("<tool_result>") {
        let rest = &text[start + "<tool_result>".len()..];
        rest[..rest.rfind("</tool_result>").unwrap()]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    } else {
        text.to_string()
    };
    serde_json::from_str(&inner).expect("result stays valid JSON")
}

// ---------------------------------------------------------------------------
// Compaction pass
// ---------------------------------------------------------------------------

#[test]
fn b2_shape_few_huge_reads_compact_below_budget_keeping_anchor_pairs_and_recent_reads() {
    let budget = 44_237; // 65,536 window, max_tokens 8,192
    let mut messages = native_history(&[
        ("src/agent/task_policy.rs", 8_000),
        ("tests/unit/agent/task_policy/task_policy_test.rs", 5_000),
        ("src/agent/turn_artifacts.rs", 7_000),
        ("src/agent/progress.rs", 8_000),
        ("src/agent/loop_control.rs", 9_000),
        ("tests/unit/agent/loop_control/loop_control_test.rs", 9_000),
    ]);
    let before_len = messages.len();
    let before_tokens = estimate_messages_tokens(&messages);
    assert!(
        before_tokens > budget,
        "precondition: {before_tokens} over {budget}"
    );
    assert!(
        messages.len() <= 14,
        "precondition: a small message count (the summary path's blind spot)"
    );
    let recent: Vec<String> = messages[messages.len() - 3..]
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();

    let report = compact_tool_results_to_budget(
        &mut messages,
        budget,
        RECENT_RESULTS_KEPT_INTACT,
        stub_token_budget(budget),
        &|_| None,
    )
    .expect("compacted");

    let after = estimate_messages_tokens(&messages);
    assert!(after <= budget, "{after} > {budget}");
    assert_eq!(report.after_tokens, after);
    assert_eq!(report.before_tokens, before_tokens);
    assert_eq!(messages.len(), before_len, "no message is dropped");
    assert_eq!(messages[1].content.text(), TASK, "task anchor untouched");
    assert_pairs_valid(&messages);
    // Oldest first: the first read went, the two most recent are intact.
    assert!(report.stubbed[0].contains("src/agent/task_policy.rs"));
    assert!(report.truncated.is_empty());
    let recent_after: Vec<String> = messages[messages.len() - 3..]
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    assert_eq!(recent_after, recent, "the most recent reads stay intact");
}

#[test]
fn c24_shape_xml_reads_with_three_messages_compact_instead_of_dropping() {
    let budget = 11_008; // 24,000 window, max_tokens 8,192
    let mut messages = vec![
        system_prompt(),
        Message::user(TASK),
        xml_call("src/agent/context.rs"),
        xml_result(&read_payload(&rust_source("ctx", 5_500))),
        xml_call("src/agent/compression.rs"),
        xml_result(&read_payload(&rust_source("cmp", 5_500))),
    ];
    let before = estimate_messages_tokens(&messages);
    assert!(before > budget);

    let report = compact_tool_results_to_budget(
        &mut messages,
        budget,
        RECENT_RESULTS_KEPT_INTACT,
        stub_token_budget(budget),
        &|_| Some("defines ContextCompressor and the work ledger".to_string()),
    )
    .expect("compacted");
    assert!(estimate_messages_tokens(&messages) <= budget);
    assert_eq!(messages.len(), 6);
    // Both results are still XML tool results in their own user messages.
    assert!(Agent::is_tool_result_user_message(&messages[3]));
    assert!(Agent::is_tool_result_user_message(&messages[5]));
    // The older read is a stub that says its content is gone.
    let stub = payload_of(&messages[3]);
    assert_eq!(stub[COMPACTED_RESULT_KEY], "stub");
    assert_eq!(stub["path"], "src/agent/context.rs");
    assert_eq!(stub["content_in_context"], false);
    assert!(stub["note"]
        .as_str()
        .unwrap()
        .contains("NO LONGER in your context"));
    assert_eq!(
        stub["findings"],
        "defines ContextCompressor and the work ledger"
    );
    assert_eq!(report.stubbed, vec!["file_read src/agent/context.rs"]);
}

#[test]
fn stub_symbols_carry_real_line_numbers_including_range_offsets() {
    let content = "use x;\n\npub fn alpha() {}\nstruct Beta;\n    async fn gamma(&self) {\n}\n";
    let stub: serde_json::Value = serde_json::from_str(&build_stub(
        "file_read",
        r#"{"path":"src/a.rs","line_range":[100,105]}"#,
        &read_payload(content),
        400,
        None,
    ))
    .unwrap();
    let symbols: Vec<&str> = stub["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(
        symbols,
        vec![
            "102: pub fn alpha() {}",
            "103: struct Beta;",
            "104: async fn gamma(&self)"
        ]
    );
    assert_eq!(stub["line_range"], serde_json::json!([100, 105]));
    assert_eq!(
        stub["content_hash"],
        format!(
            "{:016x}",
            crate::agent::context::content_fingerprint(content)
        )
    );
}

#[test]
fn a_numbered_view_supplies_its_own_line_numbers() {
    let digest = symbol_digest(
        "  41| pub fn a() {\n  42|     1\n  43| }\n  44| enum E {}\n",
        1,
    );
    assert_eq!(
        digest,
        vec![
            (41, "pub fn a()".to_string()),
            (44, "enum E {}".to_string())
        ]
    );
}

#[test]
fn stubs_stay_within_their_token_budget() {
    let payload = read_payload(&rust_source("big", 12_000));
    for budget in [200, 300, 500] {
        let stub = build_stub(
            "file_read",
            r#"{"path":"src/big.rs"}"#,
            &payload,
            budget,
            None,
        );
        assert!(
            estimate_content_tokens(&stub) <= budget,
            "stub {} > {budget}",
            estimate_content_tokens(&stub)
        );
        let v: serde_json::Value = serde_json::from_str(&stub).unwrap();
        assert!(
            v["symbols_omitted"].as_u64().unwrap() > 0,
            "omission is stated"
        );
    }
}

/// b2_65536 turn 15: one read of verification.rs (136,803 chars) next to
/// the system prompt and the task. The latest read is cut to whole leading
/// lines that fit, never stubbed, and names the lines that are NOT shown.
#[test]
fn a_single_huge_latest_read_is_cut_to_named_lines_not_dropped() {
    let budget = 20_000;
    let mut messages = native_history(&[("src/agent/verification.rs", 35_000)]);
    let report = compact_tool_results_to_budget(
        &mut messages,
        budget,
        RECENT_RESULTS_KEPT_INTACT,
        stub_token_budget(budget),
        &|_| None,
    )
    .expect("compacted");
    assert!(estimate_messages_tokens(&messages) <= budget);
    assert_eq!(
        report.truncated,
        vec!["file_read src/agent/verification.rs"]
    );
    let cut = payload_of(&messages[3]);
    assert_eq!(cut[COMPACTED_RESULT_KEY], "truncated");
    let shown = cut["shown_line_range"].as_array().unwrap();
    let shown_end = shown[1].as_u64().unwrap();
    assert_eq!(shown[0], 1);
    assert!(shown_end > 50, "keeps a useful head: {shown_end}");
    let content = cut["content"].as_str().unwrap();
    assert_eq!(content.lines().count() as u64, shown_end);
    assert!(cut["note"]
        .as_str()
        .unwrap()
        .contains(&format!("lines {}-", shown_end + 1)));
    assert!(cut["note"]
        .as_str()
        .unwrap()
        .contains("NOT in your context"));
    assert_pairs_valid(&messages);
}

#[test]
fn compaction_is_idempotent_and_never_restubs() {
    let budget = 30_000;
    let mut messages = native_history(&[
        ("src/a.rs", 8_000),
        ("src/b.rs", 8_000),
        ("src/c.rs", 8_000),
        ("src/d.rs", 8_000),
    ]);
    compact_tool_results_to_budget(&mut messages, budget, 2, 300, &|_| None).unwrap();
    let snapshot: Vec<String> = messages
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    assert!(
        compact_tool_results_to_budget(&mut messages, budget, 2, 300, &|_| None).is_none(),
        "fits already: nothing to do"
    );
    // Even when asked for less, stubs are not re-stubbed.
    let _ = compact_tool_results_to_budget(&mut messages, 1_000, 2, 300, &|_| None);
    let stub_before = &snapshot[3];
    assert_eq!(messages[3].content.text(), stub_before);
}

#[test]
fn a_fitting_history_is_left_alone() {
    let mut messages = native_history(&[("src/a.rs", 1_000)]);
    let before: Vec<String> = messages
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    assert!(compact_tool_results_to_budget(&mut messages, 100_000, 2, 300, &|_| None).is_none());
    let after: Vec<String> = messages
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------------
// Ledger and presence
// ---------------------------------------------------------------------------

#[test]
fn the_ledger_records_a_digest_before_compaction_and_never_counts_a_stub_as_a_read() {
    let mut messages = native_history(&[
        ("src/a.rs", 6_000),
        ("src/b.rs", 6_000),
        ("src/c.rs", 6_000),
    ]);
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some(TASK));
    ledger.observe(&messages, None);
    compact_tool_results_to_budget(&mut messages, 14_000, 2, 300, &|_| None).unwrap();
    ledger.begin_turn(Some(TASK));
    ledger.observe(&messages, None);
    let a = ledger
        .files()
        .iter()
        .find(|f| f.path == "src/a.rs")
        .unwrap();
    assert_eq!(a.reads, 1, "the stub is not a second read");
    assert!(
        !a.partial,
        "the stub does not turn the read into a partial one"
    );
    assert!(
        a.symbols
            .iter()
            .any(|(line, s)| *line == 4 && s == "pub fn f0_0(input: &str) -> usize"),
        "digest recorded from the full content: {:?}",
        &a.symbols[..3.min(a.symbols.len())]
    );
}

#[test]
fn presence_tracks_whole_ranges_truncated_and_stubbed_reads() {
    let mut messages = native_history(&[
        ("src/a.rs", 3_000),
        ("src/b.rs", 3_000),
        ("src/c.rs", 3_000),
    ]);
    messages.push(native_call(
        "r",
        "file_read",
        serde_json::json!({"path": "src/d.rs", "line_range": [10, 20]}),
    ));
    messages.push(Message::tool(read_payload("fn x() {}\n"), "r"));
    let _ = compact_tool_results_to_budget(&mut messages, 10_000, 1, 200, &|_| None);
    let presence = ContextPresence::from_messages(&messages, &|p| p.to_string());
    assert!(!presence.whole("src/a.rs"), "stubbed");
    assert!(presence.ranges("src/a.rs").is_empty());
    assert_eq!(presence.ranges("src/d.rs"), vec![(10, 20)]);
}

/// Replay of the b2_65536 failure: the ledger rendered for the request that
/// is actually sent must tell the model which files' content is gone (so it
/// re-reads a range instead of inventing), which is still there (so it does
/// not re-read), and never forbid re-reading content that is gone.
#[test]
fn replay_rendered_request_tells_the_model_which_content_is_gone() {
    let compressor = ContextCompressor::new(44_237);
    let mut history = native_history(&[
        ("src/agent/last_tool.rs", 6_000),
        ("src/agent/task_policy.rs", 8_000),
        ("src/agent/turn_artifacts.rs", 8_000),
        ("src/agent/progress.rs", 8_000),
        ("src/agent/loop_control.rs", 8_000),
    ]);
    compressor.begin_ledger_turn(Some(TASK));
    compressor.observe_work(&history);
    let budget = 30_000;
    compact_tool_results_to_budget(
        &mut history,
        budget,
        RECENT_RESULTS_KEPT_INTACT,
        stub_token_budget(budget),
        &|p| compressor.file_finding(p),
    )
    .unwrap();

    let request = Agent::finish_request_with_tail_and_ledger(
        history,
        vec![],
        &|h, cap| compressor.render_work_ledger_for(cap, h),
        budget + 4_000,
        None,
    )
    .expect("fits");
    let tail = request.last().unwrap().content.text().to_string();
    assert!(tail.contains("Work ledger"), "ledger attached: {tail}");
    let line_of = |path: &str| {
        tail.lines()
            .find(|l| l.starts_with(&format!("- {path} — ")))
            .unwrap_or_else(|| panic!("{path} listed in:\n{tail}"))
            .to_string()
    };
    assert!(line_of("src/agent/last_tool.rs").contains("[content NOT in context]"));
    assert!(line_of("src/agent/task_policy.rs").contains("[content NOT in context]"));
    assert!(line_of("src/agent/loop_control.rs").contains("[content in context]"));
    assert!(line_of("src/agent/progress.rs").contains("[content in context]"));
    assert!(
        tail.contains("symbols (index, not code): 4: pub fn f0_0(input: &str) -> usize"),
        "a gone file carries its digest with line numbers:\n{tail}"
    );
    assert!(tail.contains("re-read just the line range you need"));
    assert!(tail.contains("never answer from memory of content that is not in context"));
    assert!(
        !tail.contains("Do not re-read a file listed here"),
        "must never forbid re-reading content that is gone"
    );
    assert!(crate::token_count::estimate_messages_tokens(&request) <= budget + 4_000);
}

/// `trim_messages` (the request-fitting and history trim) compacts results
/// in place before it drops whole messages: on the c24 shape nothing is
/// dropped and the task anchor survives.
#[test]
fn trim_compacts_before_dropping_on_the_c24_shape() {
    let mut messages = vec![
        system_prompt(),
        Message::user(TASK),
        xml_call("src/agent/context.rs"),
        xml_result(&read_payload(&rust_source("ctx", 5_500))),
        xml_call("src/agent/compression.rs"),
        xml_result(&read_payload(&rust_source("cmp", 3_000))),
    ];
    let (dropped, saved) = Agent::trim_messages(&mut messages, 11_008, Some(1));
    assert_eq!(dropped, 0, "no message dropped");
    assert!(saved > 0);
    assert_eq!(messages.len(), 6);
    assert_eq!(messages[1].content.text(), TASK);
    assert!(estimate_messages_tokens(&messages) <= 11_008);
    assert!(
        messages[5].content.text().contains("fn cmp_0"),
        "latest read intact"
    );
}

/// Live 65,536 rerun: the auto-loaded review overview (skeletons of 30
/// files, ~17.7k tokens, coalesced with the task into one 56k-char user
/// message) stayed pinned while every read the model asked for was stubbed,
/// and the model thrashed re-reading. The overview goes first; the task and
/// the directives in the same message stay verbatim.
#[test]
fn the_auto_loaded_overview_goes_before_any_read_and_the_task_stays() {
    let mut overview = format!(
        "{REFERENCE_OVERVIEW_MARKER}\n\n\n## Codebase Overview (30 Rust files, function/struct \
         signatures)\nYou already have the full project structure below. Use `file_read` only \
         for files you need to see in full detail.\n\n"
    );
    for f in 0..30 {
        overview.push_str(&format!(
            "// tests/unit/mod_{f}_test.rs\nL1: use super::*\n"
        ));
        for i in 0..40 {
            overview.push_str(&format!("L{}: fn test_case_{f}_{i}()\n", 4 + i * 9));
        }
        overview.push('\n');
    }
    let directive =
        "<selfware_system_directive>\nSELFWARE INPUT CENSUS\n</selfware_system_directive>";
    let first = format!("{overview}\n\n{TASK}\n\n{directive}");
    let mut messages = vec![system_prompt(), Message::user(first)];
    messages.extend(native_history(&[("src/a.rs", 3_000), ("src/b.rs", 3_000)]).split_off(2));
    let before = estimate_messages_tokens(&messages);
    let reads: Vec<String> = messages[2..]
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    let report = compact_tool_results_to_budget(
        &mut messages,
        before - 2_000,
        RECENT_RESULTS_KEPT_INTACT,
        300,
        &|_| None,
    )
    .expect("compacted");
    assert_eq!(report.overviews_removed, 1);
    assert!(report.stubbed.is_empty(), "no read stubbed: {report:?}");
    let first = messages[1].content.text();
    assert!(!first.contains("fn test_case_3_7()"), "overview gone");
    assert!(first.contains("NOT in your context any more"));
    assert!(first.contains(TASK), "task verbatim");
    assert!(first.contains(directive), "directive verbatim");
    let after: Vec<String> = messages[2..]
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    assert_eq!(after, reads, "reads untouched");
    assert!(report
        .describe()
        .contains("1 auto-loaded codebase overview(s) removed"));
}
