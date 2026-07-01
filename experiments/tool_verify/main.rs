//! Verify every registered selfware tool against GLM-5.2 (or whatever model the
//! config points at).
//!
//! For each tool in the `ToolRegistry`, this forces the model to call that exact
//! tool (named `tool_choice`) and validates the arguments the model returns
//! against the tool's own JSON schema (required fields present, declared types
//! match). This is a per-tool "accuracy" check: can the model produce a valid
//! call for every tool selfware exposes?
//!
//! It reuses selfware's own tool registry and config (endpoint / model / key /
//! OpenRouter provider routing), so it exercises the real tool schemas.
//!
//! Build: cargo build --release --features tool-verify --bin tool_verify
//! Run:   source ~/.openrouter_env
//!        ./target/release/tool_verify                 # all tools
//!        ./target/release/tool_verify --limit 10      # first 10 (cheap)
//!        ./target/release/tool_verify --tool file_read
//!        ./target/release/tool_verify --json experiments/tool_verify/results/verify.json
//!
//! It does NOT execute the tools (that would run file_write/shell with
//! model-chosen args); it verifies the model emits schema-valid calls.

use std::path::PathBuf;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use serde_json::{json, Value};

use selfware::config::Config;
use selfware::tools::ToolRegistry;

const CONCURRENCY: usize = 6;

struct ToolSpec {
    name: String,
    description: String,
    schema: Value,
}

fn all_tools() -> Vec<ToolSpec> {
    let reg = ToolRegistry::new();
    let mut specs: Vec<ToolSpec> = Vec::new();
    for tool in reg.list_critical().into_iter().chain(reg.list_deferred()) {
        specs.push(ToolSpec {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            schema: tool.schema(),
        });
    }
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    specs.dedup_by(|a, b| a.name == b.name);
    specs
}

// ---------------------------------------------------------------------------
// Minimal JSON-schema conformance check (required + declared types)
// ---------------------------------------------------------------------------

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn type_matches(declared: &str, actual: &Value) -> bool {
    let a = json_type_name(actual);
    match declared {
        // integers are acceptable where a number is expected and vice-versa when
        // the value is a whole number; be lenient like most tool runtimes.
        "number" => a == "number" || a == "integer",
        "integer" => a == "integer",
        other => other == a,
    }
}

/// Returns Ok(()) if `args` satisfies `schema`'s required fields and declared
/// property types, else Err(reason).
fn validate_args(args: &Value, schema: &Value) -> std::result::Result<(), String> {
    let obj = args
        .as_object()
        .ok_or_else(|| "arguments are not a JSON object".to_string())?;

    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for req in required {
            if let Some(field) = req.as_str() {
                if !obj.contains_key(field) {
                    return Err(format!("missing required field '{field}'"));
                }
            }
        }
    }

    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (key, val) in obj {
            if let Some(prop_schema) = props.get(key) {
                if let Some(decl) = prop_schema.get("type").and_then(|t| t.as_str()) {
                    if !val.is_null() && !type_matches(decl, val) {
                        return Err(format!(
                            "field '{key}' should be {decl} but got {}",
                            json_type_name(val)
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// One verification call
// ---------------------------------------------------------------------------

struct Outcome {
    tool: String,
    passed: bool,
    detail: String,
    provider: Option<String>,
}

async fn verify_tool(
    http: &reqwest::Client,
    url: &str,
    api_key: &str,
    model: &str,
    extra_body: &serde_json::Map<String, Value>,
    spec: &ToolSpec,
) -> Outcome {
    let tool_def = json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": spec.schema,
        }
    });
    let mut body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content":
                "You are validating a tool. Call the provided tool once with \
                 realistic, schema-valid arguments for a representative use."},
            {"role": "user", "content": format!(
                "Call the `{}` tool now with valid arguments.", spec.name)}
        ],
        "tools": [tool_def],
        "tool_choice": {"type": "function", "function": {"name": spec.name}},
        "max_tokens": 512,
        "temperature": 0.2,
    });
    // Merge config extra_body (e.g. OpenRouter provider routing).
    if let Some(map) = body.as_object_mut() {
        for (k, v) in extra_body {
            map.insert(k.clone(), v.clone());
        }
    }

    let resp = match http
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return Outcome {
                tool: spec.name.clone(),
                passed: false,
                detail: format!("HTTP error: {e}"),
                provider: None,
            }
        }
    };

    let v: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return Outcome {
                tool: spec.name.clone(),
                passed: false,
                detail: format!("bad JSON response: {e}"),
                provider: None,
            }
        }
    };

    if let Some(err) = v.get("error") {
        return Outcome {
            tool: spec.name.clone(),
            passed: false,
            detail: format!("api error: {}", err),
            provider: None,
        };
    }

    let provider = v
        .get("provider")
        .and_then(|p| p.as_str())
        .map(String::from);

    let tool_calls = v
        .pointer("/choices/0/message/tool_calls")
        .and_then(|t| t.as_array());

    let Some(calls) = tool_calls.filter(|c| !c.is_empty()) else {
        return Outcome {
            tool: spec.name.clone(),
            passed: false,
            detail: "model returned no tool_calls".to_string(),
            provider,
        };
    };

    let call = &calls[0];
    let called_name = call
        .pointer("/function/name")
        .and_then(|n| n.as_str())
        .unwrap_or("");
    if called_name != spec.name {
        return Outcome {
            tool: spec.name.clone(),
            passed: false,
            detail: format!("model called '{called_name}' instead of forced tool"),
            provider,
        };
    }

    let args_str = call
        .pointer("/function/arguments")
        .and_then(|a| a.as_str())
        .unwrap_or("");
    let args: Value = match serde_json::from_str(args_str) {
        Ok(a) => a,
        Err(e) => {
            return Outcome {
                tool: spec.name.clone(),
                passed: false,
                detail: format!("arguments are not valid JSON: {e} :: {args_str}"),
                provider,
            }
        }
    };

    match validate_args(&args, &spec.schema) {
        Ok(()) => Outcome {
            tool: spec.name.clone(),
            passed: true,
            detail: format!("valid args: {}", truncate(args_str, 70)),
            provider,
        },
        Err(reason) => Outcome {
            tool: spec.name.clone(),
            passed: false,
            detail: format!("schema violation: {reason} :: {}", truncate(args_str, 70)),
            provider,
        },
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

// ---------------------------------------------------------------------------
// CLI + main
// ---------------------------------------------------------------------------

struct Args {
    limit: Option<usize>,
    only_tool: Option<String>,
    config: Option<String>,
    json_out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args {
        limit: None,
        only_tool: None,
        config: None,
        json_out: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--limit" => a.limit = it.next().and_then(|v| v.parse().ok()),
            "--tool" => a.only_tool = it.next(),
            "--config" => a.config = it.next(),
            "--json" => a.json_out = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("tool_verify [--limit N] [--tool NAME] [--config FILE] [--json FILE]");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    a
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let config = Config::load(args.config.as_deref())
        .context("loading selfware config (needs an API key for the endpoint)")?;
    let api_key = config
        .api_key
        .as_ref()
        .map(|k| k.expose().to_string())
        .context("no API key configured (set SELFWARE_API_KEY or api_key in config)")?;
    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));
    let extra_body = config.extra_body.clone().unwrap_or_default();

    let mut specs = all_tools();
    if let Some(name) = &args.only_tool {
        specs.retain(|s| &s.name == name);
    }
    if let Some(limit) = args.limit {
        specs.truncate(limit);
    }
    if specs.is_empty() {
        anyhow::bail!("no tools selected");
    }

    println!(
        "Verifying {} tools against '{}' via {}\n",
        specs.len(),
        config.model,
        config.endpoint
    );

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let outcomes: Vec<Outcome> = stream::iter(specs.iter())
        .map(|spec| {
            let http = &http;
            let url = &url;
            let api_key = &api_key;
            let model = &config.model;
            let extra_body = &extra_body;
            async move { verify_tool(http, url, api_key, model, extra_body, spec).await }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    let mut passed = 0usize;
    let mut providers = std::collections::BTreeSet::new();
    let mut rows: Vec<Value> = Vec::new();
    // Print in stable name order.
    let mut sorted: Vec<&Outcome> = outcomes.iter().collect();
    sorted.sort_by(|a, b| a.tool.cmp(&b.tool));
    for o in &sorted {
        if o.passed {
            passed += 1;
        }
        if let Some(p) = &o.provider {
            providers.insert(p.clone());
        }
        println!(
            "  {}  {:<28} {}",
            if o.passed { "PASS" } else { "FAIL" },
            o.tool,
            o.detail
        );
        rows.push(json!({
            "tool": o.tool, "passed": o.passed,
            "detail": o.detail, "provider": o.provider,
        }));
    }

    let total = sorted.len();
    println!(
        "\n{passed}/{total} tools produced schema-valid calls ({:.0}%). Providers used: {:?}",
        if total > 0 {
            passed as f64 / total as f64 * 100.0
        } else {
            0.0
        },
        providers
    );
    if passed < total {
        println!("\nFailures (tool → reason):");
        for o in sorted.iter().filter(|o| !o.passed) {
            println!("  - {}: {}", o.tool, o.detail);
        }
    }

    if let Some(path) = &args.json_out {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(
            path,
            serde_json::to_string_pretty(&json!({
                "model": config.model,
                "total": total,
                "passed": passed,
                "results": rows,
            }))?,
        )?;
        println!("\nWrote {}", path.display());
    }

    Ok(())
}
