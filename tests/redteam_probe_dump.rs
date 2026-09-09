//! One-off tooling: dump the checker's verdict for every case in a probe
//! corpus file, for offline triage joins. Ignored by default so CI never
//! runs it; invoke explicitly:
//!
//!   cargo test --test redteam_probe_dump -- --ignored --nocapture
//!
//! Reads PROBE_DUMP_INPUT (default: tests/redteam/corpus/probe_wave_all.jsonl),
//! writes PROBE_DUMP_OUTPUT (default: /home/rig/selfdev/wave_checker_verdicts.jsonl).
//! Each output line: {"id","checker":"r|a","input_sha256"}

use sha2::{Digest, Sha256};

use selfware::api::types::{ToolCall, ToolFunction};
use selfware::config::SafetyConfig;
use selfware::safety::SafetyChecker;

#[derive(serde::Deserialize)]
struct ProbeCase {
    id: String,
    tool: String,
    arguments: String,
}

fn probe_input_fingerprint(case: &ProbeCase) -> String {
    // Same compact UTF-8 JSON tuple as scripts/redteam_verdicts.py.
    let input = serde_json::to_vec(&(&case.id, &case.tool, &case.arguments)).unwrap();
    format!("{:x}", Sha256::digest(input))
}

#[test]
fn checker_receipt_fingerprint_matches_python_contract() {
    let case = ProbeCase {
        id: "fingerprint-λ".to_string(),
        tool: "file_read".to_string(),
        arguments: r#"{"path":"café.rs"}"#.to_string(),
    };
    assert_eq!(
        probe_input_fingerprint(&case),
        "624d0f82d1d144d6eb2c85ee299920c8c324ad20d2c1c201557aaf1a5588f9cd"
    );
}

#[test]
#[ignore = "triage tooling, not a gate"]
fn dump_probe_checker_verdicts() {
    let checker = SafetyChecker::new(&SafetyConfig::default());
    let probe = std::env::var("PROBE_DUMP_INPUT").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/redteam/corpus/probe_wave_all.jsonl"
        )
        .to_string()
    });
    let text = std::fs::read_to_string(&probe).expect("probe file must exist");
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
        let input_sha256 = probe_input_fingerprint(&case);
        out.push_str(
            &serde_json::json!({
                "id": case.id,
                "checker": v,
                "input_sha256": input_sha256,
            })
            .to_string(),
        );
        out.push('\n');
        n += 1;
    }
    let out_path = std::env::var("PROBE_DUMP_OUTPUT")
        .unwrap_or_else(|_| "/home/rig/selfdev/wave_checker_verdicts.jsonl".to_string());
    let out_path = std::path::Path::new(&out_path);
    if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).expect("create verdict directory");
    }
    let temporary = out_path.with_extension(format!("jsonl.{}.tmp", std::process::id()));
    std::fs::write(&temporary, out).expect("write temporary verdicts");
    std::fs::rename(&temporary, out_path).expect("atomically replace verdicts");
    eprintln!("dumped {n} checker verdicts -> {}", out_path.display());
}
