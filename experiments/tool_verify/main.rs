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

const CONCURRENCY: usize = 1;

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

fn extract_tool_call_from_response(
    v: &Value,
    expected_name: &str,
) -> Result<Option<(String, String, Value)>, String> {
    let tool_calls = v
        .pointer("/choices/0/message/tool_calls")
        .and_then(|t| t.as_array());

    if let Some(calls) = tool_calls.filter(|c| !c.is_empty()) {
        let call = &calls[0];
        let name = call
            .pointer("/function/name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let raw_args = call
            .pointer("/function/arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();
        return match serde_json::from_str(&raw_args) {
            Ok(a) => Ok(Some((name, raw_args, a))),
            Err(e) => Err(format!("arguments are not valid JSON: {e} :: {raw_args}")),
        };
    }

    for ptr in [
        "/choices/0/message/content",
        "/choices/0/message/reasoning_content",
    ] {
        let text = v.pointer(ptr).and_then(|c| c.as_str()).unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }
        let parsed = selfware::tool_parser::parse_tool_calls(text);
        if let Some(tc) = parsed.tool_calls.first() {
            let raw = serde_json::to_string(&tc.arguments).unwrap_or_default();
            return Ok(Some((tc.tool_name.clone(), raw, tc.arguments.clone())));
        }
        if let Ok(val) = serde_json::from_str::<Value>(text) {
            if let (Some(n), Some(a)) = (
                val.get("name").and_then(|x| x.as_str()),
                val.get("arguments"),
            ) {
                let arg_val = if let Some(s) = a.as_str() {
                    serde_json::from_str(s).unwrap_or_else(|_| a.clone())
                } else {
                    a.clone()
                };
                let raw = serde_json::to_string(&arg_val).unwrap_or_default();
                return Ok(Some((n.to_string(), raw, arg_val)));
            } else if val.is_object() {
                let raw = serde_json::to_string(&val).unwrap_or_default();
                return Ok(Some((expected_name.to_string(), raw, val)));
            }
        }
    }

    Ok(None)
}

fn minimal_required_schema(schema: &Value) -> Value {
    let mut out = schema.clone();
    let required: Vec<String> = out
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if let Some(props) = out.get_mut("properties").and_then(|p| p.as_object_mut()) {
        if !required.is_empty() {
            props.retain(|k, _| required.contains(k));
        }
        for val in props.values_mut() {
            if let Some(obj) = val.as_object_mut() {
                obj.remove("default");
            }
        }
    }
    out
}

fn has_multi_enum_defaults(schema: &Value) -> bool {
    let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else {
        return false;
    };
    let enums = props.values().filter(|v| v.get("enum").is_some()).count();
    let defaults = props
        .values()
        .filter(|v| v.get("default").is_some())
        .count();
    enums >= 2 && defaults >= 2
}

async fn verify_tool(
    http: &reqwest::Client,
    url: &str,
    api_key: &str,
    model: &str,
    extra_body: &serde_json::Map<String, Value>,
    spec: &ToolSpec,
) -> Outcome {
    let initial_params = if has_multi_enum_defaults(&spec.schema) {
        minimal_required_schema(&spec.schema)
    } else {
        spec.schema.clone()
    };
    let tool_def = json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": initial_params,
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
        "max_tokens": 4096,
        "temperature": 0.2,
    });
    // Merge config extra_body (e.g. OpenRouter provider routing), then force
    // enable_thinking=false so reasoning tokens do not exhaust max_tokens
    // before the forced tool call is emitted (matches the `planning` workload).
    if let Some(map) = body.as_object_mut() {
        for (k, v) in extra_body {
            map.insert(k.clone(), v.clone());
        }
        map.insert(
            "chat_template_kwargs".to_string(),
            json!({"enable_thinking": false}),
        );
    }

    let mut last_err = None;
    let mut resp_opt = None;
    for attempt in 0..3 {
        match http.post(url).bearer_auth(api_key).json(&body).send().await {
            Ok(r) if r.status().is_success() || r.status().is_client_error() => {
                resp_opt = Some(r);
                break;
            }
            Ok(r) => {
                last_err = Some(format!("HTTP status {}", r.status()));
            }
            Err(e) => {
                last_err = Some(format!("HTTP error: {e}"));
            }
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
        }
    }

    let mut v: Value = json!({});
    let mut provider: Option<String> = None;
    let mut extracted = None;

    if let Some(resp) = resp_opt {
        match resp.json::<Value>().await {
            Ok(parsed) => {
                if let Some(err) = parsed.get("error") {
                    last_err = Some(format!("api error: {err}"));
                } else {
                    provider = parsed
                        .get("provider")
                        .and_then(|p| p.as_str())
                        .map(String::from);
                    match extract_tool_call_from_response(&parsed, &spec.name) {
                        Ok(res) => extracted = res,
                        Err(detail) => {
                            return Outcome {
                                tool: spec.name.clone(),
                                passed: false,
                                detail,
                                provider,
                            }
                        }
                    }
                    v = parsed;
                }
            }
            Err(e) => {
                last_err = Some(format!("bad JSON response: {e}"));
            }
        }
    }

    // If forced tool_choice returned an empty response or a 5xx from SGLang's
    // constrained-decoding schema compiler on complex optional fields, retry
    // with the minimal required-fields schema while still validating the
    // returned arguments against the full `spec.schema`.
    if extracted.is_none() {
        let min_tool_def = json!({
            "type": "function",
            "function": {
                "name": spec.name,
                "description": spec.description,
                "parameters": minimal_required_schema(&spec.schema),
            }
        });
        let mut fallback_body = json!({
            "model": model,
            "messages": [
                {"role": "system", "content":
                    "You are validating a tool. Call the provided tool once with \
                     realistic, schema-valid arguments for a representative use."},
                {"role": "user", "content": format!(
                    "Call the `{}` tool now with valid arguments (e.g. \"src\").", spec.name)}
            ],
            "tools": [min_tool_def],
            "tool_choice": {"type": "function", "function": {"name": spec.name}},
            "max_tokens": 1024,
            "temperature": 0.2,
        });
        if let Some(map) = fallback_body.as_object_mut() {
            for (k, val) in extra_body {
                map.insert(k.clone(), val.clone());
            }
            map.insert(
                "chat_template_kwargs".to_string(),
                json!({"enable_thinking": false}),
            );
        }
        if let Ok(resp2) = http
            .post(url)
            .bearer_auth(api_key)
            .json(&fallback_body)
            .send()
            .await
        {
            if let Ok(v2) = resp2.json::<Value>().await {
                if v2.get("error").is_none() {
                    if provider.is_none() {
                        provider = v2
                            .get("provider")
                            .and_then(|p| p.as_str())
                            .map(String::from);
                    }
                    if let Ok(Some(found)) = extract_tool_call_from_response(&v2, &spec.name) {
                        extracted = Some(found);
                    } else {
                        v = v2;
                    }
                }
            }
        }
    }

    if extracted.is_none() {
        if let Some(err) = last_err {
            return Outcome {
                tool: spec.name.clone(),
                passed: false,
                detail: err,
                provider,
            };
        }
    }

    let (called_name, args_str, args) = match extracted {
        Some(triple) => triple,
        None => {
            let content = v
                .pointer("/choices/0/message/content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .trim();
            let finish_reason = v
                .pointer("/choices/0/finish_reason")
                .and_then(|f| f.as_str())
                .unwrap_or("unknown");
            return Outcome {
                tool: spec.name.clone(),
                passed: false,
                detail: format!(
                    "model returned no tool_calls (finish_reason={finish_reason}, content={})",
                    truncate(content, 80)
                ),
                provider,
            };
        }
    };

    if called_name != spec.name {
        return Outcome {
            tool: spec.name.clone(),
            passed: false,
            detail: format!("model called '{called_name}' instead of forced tool"),
            provider,
        };
    }

    match validate_args(&args, &spec.schema) {
        Ok(()) => Outcome {
            tool: spec.name.clone(),
            passed: true,
            detail: format!("valid args: {}", truncate(&args_str, 70)),
            provider,
        },
        Err(reason) => Outcome {
            tool: spec.name.clone(),
            passed: false,
            detail: format!("schema violation: {reason} :: {}", truncate(&args_str, 70)),
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
    offset: usize,
    limit: Option<usize>,
    prefix: Option<String>,
    only_tool: Option<String>,
    concurrency: usize,
    config: Option<String>,
    json_out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args {
        offset: 0,
        limit: None,
        prefix: None,
        only_tool: None,
        concurrency: CONCURRENCY,
        config: None,
        json_out: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--offset" => a.offset = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--limit" => a.limit = it.next().and_then(|v| v.parse().ok()),
            "--prefix" => a.prefix = it.next(),
            "--tool" => a.only_tool = it.next(),
            "--concurrency" => {
                // Forced `tool_choice` compiles a distinct JSON grammar per tool on
                // SGLang/Colibri; >2 concurrent grammar compilations can exhaust or
                // wedge the engine.
                a.concurrency = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(CONCURRENCY)
                    .clamp(1, 2)
            }
            "--config" => a.config = it.next(),
            "--json" => a.json_out = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!(
                    "tool_verify [--offset N] [--limit N] [--prefix STR] [--tool NAME] [--concurrency N] [--config FILE] [--json FILE]"
                );
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
    if let Some(prefix) = &args.prefix {
        specs.retain(|s| s.name.starts_with(prefix));
    }
    if args.offset > 0 {
        specs = specs.into_iter().skip(args.offset).collect();
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
        .buffer_unordered(args.concurrency)
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
