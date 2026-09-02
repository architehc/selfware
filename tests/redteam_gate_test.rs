//! Red-team corpus gate test.
//!
//! Every JSONL line in tests/redteam/corpus/*.jsonl is one attack (or benign
//! control) case driven through `SafetyChecker::check_tool_call` with the
//! default safety config. Attack cases (`"expect": "refuse"`) MUST be
//! rejected by the gate — a passing attack is a security hole and fails this
//! test. Benign controls (`"expect": "allow"`) guard against false positives
//! that would break normal agent work.
//!
//! Corpus files whose names start with `probe_` are skipped: those are
//! unclassified candidates produced by scripts/redteam_gen.py awaiting
//! triage, not assertions.
//!
//! The corpus is extended by the local uncensored red-team model
//! (scripts/redteam_gen.py); generated cases are committed so CI needs no
//! access to the LAN endpoint.

use selfware::api::types::{ToolCall, ToolFunction};
use selfware::config::SafetyConfig;
use selfware::safety::SafetyChecker;

#[derive(serde::Deserialize)]
struct RedteamCase {
    id: String,
    class: String,
    tool: String,
    /// JSON-encoded tool arguments (string, matching the ToolCall wire shape).
    arguments: String,
    expect: String,
    #[allow(dead_code)]
    note: String,
}

fn load_corpus() -> Vec<RedteamCase> {
    let corpus_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/redteam/corpus");
    let mut cases = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(corpus_dir)
        .expect("corpus dir must exist")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .filter(|p| {
            !p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("probe_")
        })
        .collect();
    entries.sort();
    for path in entries {
        let text = std::fs::read_to_string(&path).unwrap();
        for (lineno, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let case: RedteamCase = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("{}:{}: invalid JSONL case: {e}", path.display(), lineno + 1)
            });
            // The arguments field must itself be valid JSON — a malformed
            // attack string would test serde, not the gate.
            serde_json::from_str::<serde_json::Value>(&case.arguments).unwrap_or_else(|e| {
                panic!(
                    "{}:{}: case {} has invalid arguments JSON: {e}",
                    path.display(),
                    lineno + 1,
                    case.id
                )
            });
            cases.push(case);
        }
    }
    cases
}

#[test]
fn redteam_corpus_respects_gate_expectations() {
    let checker = SafetyChecker::new(&SafetyConfig::default());
    let cases = load_corpus();
    assert!(
        cases.len() >= 20,
        "seed corpus should have >= 20 cases, found {}",
        cases.len()
    );

    let mut failures = Vec::new();
    for case in &cases {
        let call = ToolCall {
            id: format!("redteam-{}", case.id),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: case.tool.clone(),
                arguments: case.arguments.clone(),
            },
        };
        let verdict = checker.check_tool_call(&call);
        match case.expect.as_str() {
            "refuse" => {
                if verdict.is_ok() {
                    // SECURITY HOLE: an attack the gate let through.
                    failures.push(format!(
                        "{} [{}]: attack was ALLOWED (tool={}, args={})",
                        case.id, case.class, case.tool, case.arguments
                    ));
                }
            }
            "allow" => {
                if let Err(e) = verdict {
                    // FALSE POSITIVE: benign work the gate blocked.
                    failures.push(format!(
                        "{} [{}]: benign control was REFUSED: {e} (tool={}, args={})",
                        case.id, case.class, case.tool, case.arguments
                    ));
                }
            }
            other => failures.push(format!("{}: unknown expect value {other:?}", case.id)),
        }
    }

    assert!(
        failures.is_empty(),
        "{} red-team case(s) violated gate expectations:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
