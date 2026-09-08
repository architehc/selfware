//! One-off tooling: dump the checker's verdict for every case in a probe
//! corpus file, for offline triage joins. Ignored by default so CI never
//! runs it; invoke explicitly:
//!
//!   cargo test --test redteam_probe_dump -- --ignored --nocapture
//!
//! Writes /home/rig/selfdev/probe_checker_verdicts.jsonl: {"id","checker":"r|a"}

use selfware::api::types::{ToolCall, ToolFunction};
use selfware::config::SafetyConfig;
use selfware::safety::SafetyChecker;

#[derive(serde::Deserialize)]
struct ProbeCase {
    id: String,
    tool: String,
    arguments: String,
}

#[test]
#[ignore = "triage tooling, not a gate"]
fn dump_probe_checker_verdicts() {
    let checker = SafetyChecker::new(&SafetyConfig::default());
    let probe = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/redteam/corpus/probe_backlog_waves1292plus.jsonl"
    );
    let text = std::fs::read_to_string(probe).expect("probe file must exist");
    let mut out = String::new();
    let mut n = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let case: ProbeCase = serde_json::from_str(line).expect("valid probe case");
        let call = ToolCall {
            id: format!("probe-{}", case.id),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: case.tool.clone(),
                arguments: case.arguments.clone(),
            },
        };
        let v = if checker.check_tool_call(&call).is_ok() {
            "a"
        } else {
            "r"
        };
        out.push_str(&format!(
            "{{\"id\":\"{}\",\"checker\":\"{}\"}}\n",
            case.id, v
        ));
        n += 1;
    }
    std::fs::write("/home/rig/selfdev/probe_checker_verdicts.jsonl", out)
        .expect("write verdicts");
    eprintln!("dumped {n} checker verdicts");
}
