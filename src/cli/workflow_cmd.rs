use anyhow::Result;
use tracing::warn;

use crate::config::{Config, SafetyConfig};
use crate::ui::components::{render_header, WorkshopContext};
use crate::ui::style::{Glyphs, SelfwareStyle};
use crate::workflows::{LlmCallOutput, LlmTokenUsage, VarValue, WorkflowExecutor};

use super::args::WorkflowCommands;

pub(crate) const DEFAULT_WORKFLOW_NAME: &str = "default";
const MAX_WORKFLOW_AGENT_TOOL_ROUNDS: usize = 6;

/// Workflow file kind for determining how to handle workflow files
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowFileKind {
    Swl,
    Yaml,
}

/// Determine the workflow file kind from a path
pub(crate) fn workflow_file_kind(path: &std::path::Path) -> Option<WorkflowFileKind> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("swl") => Some(WorkflowFileKind::Swl),
        Some("yaml") | Some("yml") => Some(WorkflowFileKind::Yaml),
        _ => None,
    }
}

/// Parse workflow input variables in KEY=VALUE format
pub(crate) fn parse_workflow_inputs(
    values: &[String],
) -> Result<std::collections::HashMap<String, VarValue>> {
    let mut inputs = std::collections::HashMap::new();
    for kv in values {
        if let Some((k, v)) = kv.split_once('=') {
            inputs.insert(k.to_string(), VarValue::String(v.to_string()));
        } else {
            anyhow::bail!("Invalid input format '{}', expected KEY=VALUE", kv);
        }
    }
    Ok(inputs)
}

pub(crate) fn workflow_llm_output_from_response(
    response: crate::api::ChatResponse,
) -> Result<LlmCallOutput> {
    let usage = LlmTokenUsage {
        prompt_tokens: response.usage.prompt_tokens as u64,
        completion_tokens: response.usage.completion_tokens as u64,
        total_tokens: response.usage.total_tokens as u64,
    };
    // Provider-reported only; `None` stays uncosted (never priced from
    // token counts).
    let reported_cost_usd = response.usage.cost;
    let model = response.model.clone();

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("model returned no choices"))?;
    let text = choice.message.content.text_all();
    let content = if !text.trim().is_empty() {
        text
    } else if let Some(reasoning) = choice
        .reasoning_content
        .or(choice.message.reasoning_content)
    {
        reasoning
    } else {
        anyhow::bail!("model returned empty content");
    };

    Ok(LlmCallOutput::text(content)
        .with_model(model)
        .with_usage(usage)
        .with_reported_cost(reported_cost_usd))
}

pub(crate) fn print_workflow_telemetry(result: &crate::workflows::WorkflowResult) {
    if result.telemetry.llm_calls == 0 {
        return;
    }

    println!("\n   {} Telemetry:", Glyphs::journal());
    println!(
        "      LLM calls: {}",
        result.telemetry.llm_calls.to_string().garden_healthy()
    );
    println!(
        "      Tokens: {} in / {} out / {} total",
        result.telemetry.prompt_tokens,
        result.telemetry.completion_tokens,
        result.telemetry.total_tokens
    );
    println!("      {}", workflow_cost_line(&result.telemetry));
    println!("      LLM latency: {}ms", result.telemetry.llm_latency_ms);
}

/// Print the id of a checkpointed workflow run before it starts.
pub(crate) fn print_workflow_run_id(run: &crate::workflows::WorkflowRun) {
    let verb = if run.resume_from.is_some() {
        "Resuming run"
    } else {
        "Run id"
    };
    println!(
        "   {} {}: {}",
        Glyphs::journal(),
        verb,
        run.run_id.as_str().emphasis()
    );
}

/// After a failed run, name the command that resumes it — only when the
/// checkpoint actually exists (never promise a resume that cannot work).
pub(crate) fn print_workflow_resume_hint(result: &crate::workflows::WorkflowResult, file: &str) {
    let Some(run_id) = result.run_id.as_deref() else {
        return;
    };
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let store = crate::workflows::CheckpointStore::for_workspace(&cwd);
    if store.path_for(run_id).is_ok_and(|path| path.is_file()) {
        println!(
            "   {} Resume with: selfware workflow run {} --resume {}",
            Glyphs::compass(),
            file,
            run_id
        );
    }
}

/// The workflow telemetry cost line: a dollar figure only when the provider
/// reported one ("Cost: cost $0.0123", "Cost: known cost $… (incomplete …)",
/// "Cost: cost not tracked …").
pub(crate) fn workflow_cost_line(telemetry: &crate::workflows::WorkflowTelemetry) -> String {
    let phrase = telemetry.cost_phrase();
    let mut chars = phrase.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => phrase,
    }
}

pub(crate) fn workflow_agent_label(prompt: &str) -> String {
    prompt
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("SWL agent: "))
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or("workflow_llm")
        .to_string()
}

/// Parse the comma-separated `Requested tools: ...` line emitted by SWL agent lowering.
pub(crate) fn workflow_requested_tools(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .find_map(|line| line.strip_prefix("Requested tools:"))
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn build_workflow_llm_handler_with_safety(
    client: std::sync::Arc<crate::api::ApiClient>,
    model_label: String,
    safety_config: &SafetyConfig,
) -> crate::workflows::LlmHandler {
    use crate::api::types::{FunctionDefinition, Message, ToolCall, ToolDefinition, ToolFunction};
    use crate::safety::checker::SafetyChecker;
    use crate::tools::ToolRegistry;
    use std::sync::Arc;

    let registry = Arc::new(ToolRegistry::with_safety_config(Some(safety_config)));
    let checker = Arc::new(SafetyChecker::new(safety_config));

    Box::new(move |prompt, ctx| {
        let client = Arc::clone(&client);
        let registry = Arc::clone(&registry);
        let checker = Arc::clone(&checker);
        let model_label = model_label.clone();
        let agent_label = workflow_agent_label(prompt);
        let requested_tools = workflow_requested_tools(prompt);
        let request = if ctx.is_empty() {
            prompt.to_string()
        } else {
            format!("{prompt}\n\nContext:\n{}", ctx.join("\n"))
        };

        // Native async — no block_in_place/block_on bridge, so the workflow
        // step's select! timeout can preempt (and drop) the in-flight request.
        Box::pin(async move {
            let tool_defs: Vec<ToolDefinition> = requested_tools
                .iter()
                .filter_map(|name| {
                    registry.get(name).map(|tool| ToolDefinition {
                        def_type: "function".to_string(),
                        function: FunctionDefinition {
                            name: tool.name().to_string(),
                            description: tool.description().to_string(),
                            parameters: tool.schema(),
                        },
                    })
                })
                .collect();

            let mut messages = vec![Message::user(request)];
            let mut total_usage = LlmTokenUsage::default();
            let mut total_cost: Option<f64> = None;
            let max_rounds = if tool_defs.is_empty() {
                1
            } else {
                MAX_WORKFLOW_AGENT_TOOL_ROUNDS
            };

            for round in 0..max_rounds {
                crate::observability::telemetry::increment_api_requests();

                let send_tools = if !tool_defs.is_empty() && round + 1 < max_rounds {
                    Some(tool_defs.clone())
                } else {
                    None
                };

                let started = std::time::Instant::now();
                let response = client
                    .chat(
                        messages.clone(),
                        send_tools,
                        crate::api::ThinkingMode::Disabled,
                    )
                    .await;
                let latency_ms = started.elapsed().as_secs_f64() * 1000.0;

                let response = match response {
                    Ok(response) => {
                        let prompt_tokens = response.usage.prompt_tokens as u64;
                        let completion_tokens = response.usage.completion_tokens as u64;
                        let total_tokens = response.usage.total_tokens as u64;
                        let reported_cost_usd = response.usage.cost;

                        total_usage.prompt_tokens =
                            total_usage.prompt_tokens.saturating_add(prompt_tokens);
                        total_usage.completion_tokens = total_usage
                            .completion_tokens
                            .saturating_add(completion_tokens);
                        total_usage.total_tokens =
                            total_usage.total_tokens.saturating_add(total_tokens);
                        if let Some(cost) = reported_cost_usd {
                            *total_cost.get_or_insert(0.0) += cost;
                        }

                        tracing::info!(
                            model = %model_label,
                            agent = %agent_label,
                            round,
                            latency_ms,
                            prompt_tokens,
                            completion_tokens,
                            total_tokens,
                            ?reported_cost_usd,
                            "workflow llm request completed"
                        );
                        response
                    }
                    Err(err) => {
                        crate::observability::telemetry::increment_api_errors();
                        tracing::warn!(
                            model = %model_label,
                            agent = %agent_label,
                            round,
                            latency_ms,
                            error = %err,
                            "workflow llm request failed"
                        );
                        return Err(err);
                    }
                };

                // Extract tool calls if tools are enabled for this agent and we have rounds left
                let extracted_calls: Vec<ToolCall> =
                    if !tool_defs.is_empty() && round + 1 < max_rounds {
                        if let Some(choice) = response.choices.first() {
                            if let Some(native_calls) =
                                choice.message.tool_calls.as_ref().filter(|c| !c.is_empty())
                            {
                                native_calls.clone()
                            } else {
                                let text = choice.message.content.text_all();
                                let parsed = crate::tool_parser::parse_tool_calls(&text);
                                parsed
                                    .tool_calls
                                    .into_iter()
                                    .enumerate()
                                    .map(|(idx, tc)| ToolCall {
                                        id: format!("swl-call-{round}-{idx}"),
                                        call_type: "function".to_string(),
                                        function: ToolFunction {
                                            name: tc.tool_name,
                                            arguments: tc.arguments.to_string(),
                                        },
                                    })
                                    .collect()
                            }
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };

                if extracted_calls.is_empty() {
                    let mut out = workflow_llm_output_from_response(response)?;
                    out.usage = Some(total_usage);
                    out.cost_usd = total_cost;
                    return Ok(out);
                }

                // Append assistant message with tool_calls
                let assistant_text = response
                    .choices
                    .first()
                    .map(|c| c.message.content.text_all())
                    .unwrap_or_default();
                let mut assistant_msg = Message::assistant(assistant_text);
                assistant_msg.tool_calls = Some(extracted_calls.clone());
                messages.push(assistant_msg);

                // Execute each requested tool call against the allowlist and safety checker
                for mut call in extracted_calls {
                    let tool_name = call.function.name.clone();
                    let tool_output = if !requested_tools.iter().any(|t| t == &tool_name) {
                        serde_json::json!({
                            "error": format!(
                                "Tool '{}' is not permitted for SWL agent '{}' (allowed: {})",
                                tool_name,
                                agent_label,
                                requested_tools.join(", ")
                            )
                        })
                        .to_string()
                    } else {
                        match parse_workflow_tool_arguments(&tool_name, &call.function.arguments) {
                            Err(parse_err) => {
                                tracing::warn!(
                                    agent = %agent_label,
                                    tool = %tool_name,
                                    error = %parse_err,
                                    "SWL agent emitted malformed tool arguments"
                                );
                                serde_json::json!({ "error": parse_err }).to_string()
                            }
                            Ok(args) => {
                                call.function.arguments = args.to_string();
                                if let Err(e) = checker.check_tool_call(&call) {
                                    tracing::warn!(
                                        agent = %agent_label,
                                        tool = %tool_name,
                                        error = %e,
                                        "SWL agent tool call blocked by safety policy"
                                    );
                                    serde_json::json!({
                                        "error": format!("Safety policy blocked tool '{}': {}", tool_name, e)
                                    })
                                    .to_string()
                                } else {
                                    match registry.execute_any(&tool_name, args).await {
                                        Ok(val) => val.to_string(),
                                        Err(err) => serde_json::json!({
                                            "error": format!("Tool '{}' execution failed: {}", tool_name, err)
                                        })
                                        .to_string(),
                                    }
                                }
                            }
                        }
                    };
                    messages.push(Message::tool(tool_output, call.id));
                }
            }

            anyhow::bail!(
                "SWL agent '{}' exceeded max tool rounds ({})",
                agent_label,
                max_rounds
            )
        })
    })
}

/// Parse tool arguments for an SWL workflow agent tool call.
///
/// Empty or JSON `null` arguments normalize to `{}`; malformed JSON or
/// non-object JSON values (arrays, numbers, booleans, strings) return `Err`
/// rather than silently coercing to `{}` and running the tool with default
/// arguments.
pub(crate) fn parse_workflow_tool_arguments(
    tool_name: &str,
    raw_arguments: &str,
) -> std::result::Result<serde_json::Value, String> {
    let trimmed = raw_arguments.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::json!({}));
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Object(map)) => Ok(serde_json::Value::Object(map)),
        Ok(serde_json::Value::Null) => Ok(serde_json::json!({})),
        Ok(other) => {
            let kind = match other {
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                _ => "non-object",
            };
            Err(format!(
                "Invalid arguments for tool '{}': expected JSON object, got {}",
                tool_name, kind
            ))
        }
        Err(err) => Err(format!(
            "Invalid JSON arguments for tool '{}': {}",
            tool_name, err
        )),
    }
}

/// Build a [`ToolHandler`](crate::workflows::ToolHandler) that dispatches Tool
/// steps to the real [`ToolRegistry`] behind the safety gate.
///
/// The handler converts the `HashMap<String, String>` args from the workflow
/// engine into a `serde_json::Value::Object` and returns a future that
/// `.await`s the registry's async `execute_any` call natively (no
/// `block_in_place` bridge), so the workflow step timeout can preempt it.
///
/// `execute_any` is used (instead of `execute`) so that deferred tools — which
/// are common in workflow scripts (e.g. `git_status`, `cargo_test`) — are also
/// reachable without requiring an explicit activation step.
pub(crate) fn build_workflow_tool_handler(
    safety_config: &crate::config::SafetyConfig,
) -> crate::workflows::ToolHandler {
    use crate::tools::ToolRegistry;
    use std::collections::HashMap;
    use std::sync::Arc;

    // Build the registry with the same safety config used elsewhere in the CLI.
    let registry = Arc::new(ToolRegistry::with_safety_config(Some(safety_config)));
    // The registry's execute_any is raw dispatch; the POLICY decision belongs
    // here at the front door. Build the same central checker the main agent
    // uses so a call rejected in chat is rejected in a workflow (review
    // finding: workflow tool steps bypassed the primary safety gate).
    let checker = Arc::new(crate::safety::checker::SafetyChecker::new(safety_config));

    Box::new(move |tool_name: &str, args: &HashMap<String, String>| {
        let registry = Arc::clone(&registry);
        let checker = Arc::clone(&checker);
        let tool_name = tool_name.to_string();
        let args = args.clone();

        Box::pin(async move {
            // Convert HashMap<String, String> → serde_json::Value::Object
            let mut json_map = serde_json::Map::new();
            for (k, v) in &args {
                // Try to parse each value as JSON first (so numbers/bools/arrays
                // pass through correctly); fall back to a plain string.
                let parsed = serde_json::from_str::<serde_json::Value>(v)
                    .unwrap_or(serde_json::Value::String(v.clone()));
                json_map.insert(k.clone(), parsed);
            }
            let input = serde_json::Value::Object(json_map);

            // Central safety gate — identical to the main agent's check.
            let call = crate::api::types::ToolCall {
                id: format!("workflow-{tool_name}"),
                call_type: "function".to_string(),
                function: crate::api::types::ToolFunction {
                    name: tool_name.clone(),
                    arguments: input.to_string(),
                },
            };
            if let Err(e) = checker.check_tool_call(&call) {
                tracing::warn!(tool = %tool_name, error = %e, "workflow tool step blocked by safety policy");
                return Err(e.into());
            }

            tracing::info!(
                tool = %tool_name,
                args = ?input,
                "workflow tool_handler dispatching to ToolRegistry"
            );

            // Native async dispatch — no block_in_place bridge, so the
            // workflow's select! timeout can actually preempt this step.
            let result = registry.execute_any(&tool_name, input).await;

            match result {
                Ok(value) => {
                    // A Rust Ok means "the tool returned a response", not "the
                    // requested operation succeeded". shell_exec encodes process
                    // failure in the payload (`{"exit_code": 7, …}`); other tools
                    // report `success: false`. Interpret both so a failed
                    // operation fails the workflow step (review finding: tool
                    // failures were reported as completed steps).
                    if let Some(code) = value.get("exit_code").and_then(|v| v.as_i64()) {
                        if code != 0 {
                            let stderr = value.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                            return Err(anyhow::anyhow!(
                                "tool '{tool_name}' exited with code {code}: {stderr}"
                            ));
                        }
                    }
                    if value.get("success").and_then(|v| v.as_bool()) == Some(false) {
                        let err = value
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        return Err(anyhow::anyhow!("tool '{tool_name}' failed: {err}"));
                    }
                    Ok(value.to_string())
                }
                Err(err) => {
                    tracing::warn!(tool = %tool_name, error = %err, "workflow tool execution failed");
                    Err(err)
                }
            }
        })
    })
}

pub(crate) fn default_workflow_name(path: &std::path::Path) -> String {
    match path.file_stem().and_then(|s| s.to_str()) {
        Some(name) => name.to_string(),
        None => {
            warn!(
                "Could not infer workflow name from file '{}'; using '{}'",
                path.display(),
                DEFAULT_WORKFLOW_NAME
            );
            DEFAULT_WORKFLOW_NAME.to_string()
        }
    }
}

/// Directory whose YAML workflows are loaded next to `path`. A bare file
/// name has the EMPTY parent `""`, which walkdir cannot open ("IO error for
/// operation on : No such file or directory" for `workflow run wf.yaml`);
/// that means the current directory.
pub(crate) fn related_workflow_dir(path: &std::path::Path) -> &std::path::Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
}

pub(crate) fn load_related_yaml_workflows(
    executor: &mut WorkflowExecutor,
    path: &std::path::Path,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(related_workflow_dir(path)).max_depth(1) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let workflow_path = entry.path();
        if workflow_file_kind(workflow_path) == Some(WorkflowFileKind::Yaml) {
            executor.load_file(workflow_path)?;
        }
    }

    Ok(())
}

pub(crate) async fn handle_workflow_command(
    command: WorkflowCommands,
    config: &Config,
    ctx: &WorkshopContext,
    quiet: bool,
) -> Result<()> {
    use crate::swl::{parse_document, validate_document};

    if !quiet {
        println!("{}", render_header(ctx));
    }

    match command {
        WorkflowCommands::Validate { file } => {
            let path = std::path::Path::new(&file);
            if !path.exists() {
                anyhow::bail!("Workflow file not found: {}", file);
            }

            match workflow_file_kind(path) {
                Some(WorkflowFileKind::Swl) => {
                    println!(
                        "\n{} {}\n",
                        Glyphs::gear(),
                        "SWL Validation".workshop_title()
                    );
                    println!(
                        "   {} File: {}",
                        Glyphs::journal(),
                        file.as_str().path_local()
                    );

                    let source = super::subcommands::read_bounded_cli_file(
                        path,
                        4 * 1024 * 1024,
                        "SWL workflow file",
                    )?;
                    let doc = match parse_document(&source) {
                        Ok(d) => d,
                        Err(e) => {
                            println!("\n   {} Validation failed:\n", Glyphs::frost());
                            println!("   {}\n", e);
                            anyhow::bail!("SWL validation failed");
                        }
                    };

                    let issues = validate_document(&doc);
                    if issues.is_empty() {
                        println!("\n   {} SWL file is valid!", Glyphs::bloom());
                        println!("   Name: {}", doc.name.emphasis());
                        println!("   Version: {}", doc.version);
                        println!(
                            "   Agents: {}",
                            doc.agents.len().to_string().garden_healthy()
                        );
                        println!(
                            "   Workflows: {}",
                            doc.workflows.len().to_string().garden_healthy()
                        );
                        println!();
                    } else {
                        println!("\n   {} Validation issues found:\n", Glyphs::frost());
                        for issue in &issues {
                            println!(
                                "   - {}: {}",
                                issue.path.clone().path_local(),
                                issue.message
                            );
                        }
                        println!();
                        anyhow::bail!("SWL validation failed with {} issues", issues.len());
                    }
                }
                Some(WorkflowFileKind::Yaml) => {
                    println!(
                        "\n{} {}\n",
                        Glyphs::gear(),
                        "YAML Workflow Validation".workshop_title()
                    );
                    println!(
                        "   {} File: {}",
                        Glyphs::journal(),
                        file.as_str().path_local()
                    );

                    let source = super::subcommands::read_bounded_cli_file(
                        path,
                        4 * 1024 * 1024,
                        "YAML workflow file",
                    )?;
                    let workflow: crate::workflows::Workflow = serde_yaml::from_str(&source)
                        .map_err(|e| anyhow::anyhow!("Failed to parse workflow YAML: {}", e))?;

                    let mut executor = WorkflowExecutor::new_with_config(&config.safety);
                    executor.load_file(path)?;

                    println!("\n   {} YAML workflow is valid!", Glyphs::bloom());
                    println!("   Name: {}", workflow.name.emphasis());
                    println!("   Version: {}", workflow.version);
                    println!(
                        "   Steps: {}",
                        workflow.steps.len().to_string().garden_healthy()
                    );
                    println!(
                        "   Inputs: {}",
                        workflow.inputs.len().to_string().garden_healthy()
                    );
                    println!();
                }
                None => anyhow::bail!(
                    "Unsupported workflow file '{}'. Use .swl, .yaml, or .yml",
                    file
                ),
            }
        }

        WorkflowCommands::Codegen { file } => {
            if !file.exists() {
                anyhow::bail!("Workflow file not found: {}", file.display());
            }

            let source = super::subcommands::read_bounded_cli_file(
                &file,
                4 * 1024 * 1024,
                "SWL workflow file",
            )?;
            let doc = match parse_document(&source) {
                Ok(d) => d,
                Err(e) => {
                    anyhow::bail!("Failed to parse SWL file {}: {}", file.display(), e);
                }
            };

            print!("{}", crate::swl::codegen::generate_rust_stub(&doc));
        }

        WorkflowCommands::Run {
            file,
            workflow,
            input,
            dry_run,
            resume,
        } => {
            let path = std::path::Path::new(&file);
            // Live runs are checkpointed per step so a failed or
            // budget-stopped run can be resumed (`--resume`).
            let checkpoint_store =
                crate::workflows::CheckpointStore::for_workspace(&std::env::current_dir()?);
            let resumed_run = resume
                .as_deref()
                .map(|run_id| {
                    crate::workflows::WorkflowRun::resume(checkpoint_store.clone(), run_id)
                })
                .transpose()?;
            if !path.exists() {
                anyhow::bail!("Workflow file not found: {}", file);
            }

            let inputs = parse_workflow_inputs(&input)?;

            match workflow_file_kind(path) {
                Some(WorkflowFileKind::Swl) => {
                    use crate::swl::{lower_document, parse_document};

                    let source = super::subcommands::read_bounded_cli_file(
                        path,
                        4 * 1024 * 1024,
                        "SWL workflow file",
                    )?;
                    let doc = match parse_document(&source) {
                        Ok(d) => d,
                        Err(e) => {
                            anyhow::bail!("Failed to parse SWL file: {}", e);
                        }
                    };

                    let lowered = lower_document(&doc)?;
                    let workflow_name = workflow
                        .clone()
                        .or_else(|| {
                            resumed_run
                                .as_ref()
                                .and_then(|run| run.resumed_workflow_name())
                                .map(str::to_string)
                        })
                        .or_else(|| lowered.workflows.first().map(|w| w.name.clone()))
                        .or_else(|| doc.workflows.keys().next().cloned())
                        .unwrap_or_else(|| "main".to_string());

                    if dry_run {
                        println!(
                            "\n{} {} (dry-run mode)\n",
                            Glyphs::gear(),
                            "SWL Workflow".workshop_title()
                        );
                        println!(
                            "   {} File: {}",
                            Glyphs::journal(),
                            file.as_str().path_local()
                        );
                        if !inputs.is_empty() {
                            println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                        }
                        if !lowered.warnings.is_empty() {
                            println!("   {} Lowering warnings:", Glyphs::fallen_leaf());
                            for warning in &lowered.warnings {
                                println!("      - {}", warning);
                            }
                        }
                        println!();

                        let mut executor =
                            WorkflowExecutor::new_dry_run_with_config(&config.safety);
                        lowered.register_into(&mut executor);

                        let working_dir = std::env::current_dir()?;
                        let result = executor
                            .execute(&workflow_name, inputs, working_dir)
                            .await?;
                        if result.status == crate::orchestration::workflows::WorkflowStatus::Failed
                        {
                            let failed: Vec<String> = result
                                .step_results
                                .iter()
                                .filter(|(_, s)| {
                                    s.status == crate::orchestration::workflows::StepStatus::Failed
                                })
                                .map(|(name, s)| {
                                    format!(
                                        "{}: {}",
                                        name,
                                        s.error.as_deref().unwrap_or("step failed")
                                    )
                                })
                                .collect();
                            anyhow::bail!(
                                "SWL workflow '{}' failed (dry-run) after {}ms ({})",
                                workflow_name,
                                result.duration_ms,
                                if failed.is_empty() {
                                    "no step details".to_string()
                                } else {
                                    failed.join("; ")
                                }
                            );
                        }
                        println!(
                            "   {} Workflow completed (dry-run) in {}ms",
                            Glyphs::bloom(),
                            result.duration_ms
                        );
                        if !result.outputs.is_empty() {
                            println!("   {} Outputs:", Glyphs::journal());
                            for (name, value) in &result.outputs {
                                println!("      {} = {:?}", name.as_str().emphasis(), value);
                            }
                        }
                        println!();
                    } else {
                        println!("\n{} {}\n", Glyphs::gear(), "SWL Workflow".workshop_title());
                        println!(
                            "   {} File: {}",
                            Glyphs::journal(),
                            file.as_str().path_local()
                        );
                        if !inputs.is_empty() {
                            println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                        }
                        if !lowered.warnings.is_empty() {
                            println!("   {} Lowering warnings:", Glyphs::fallen_leaf());
                            for warning in &lowered.warnings {
                                println!("      - {}", warning);
                            }
                        }
                        println!();

                        let mut executor = WorkflowExecutor::new_with_config(&config.safety);
                        lowered.register_into(&mut executor);

                        let client = std::sync::Arc::new(crate::api::ApiClient::new(config)?);
                        executor = executor.with_async_llm_handler(
                            build_workflow_llm_handler_with_safety(
                                client,
                                config.model.clone(),
                                &config.safety,
                            ),
                        );
                        executor =
                            executor.with_tool_handler(build_workflow_tool_handler(&config.safety));

                        println!(
                            "   {} Executing workflow: {}\n",
                            Glyphs::compass(),
                            workflow_name.as_str().emphasis()
                        );

                        let working_dir = std::env::current_dir()?;
                        let run = resumed_run.clone().unwrap_or_else(|| {
                            crate::workflows::WorkflowRun::fresh(
                                checkpoint_store.clone(),
                                &workflow_name,
                            )
                        });
                        print_workflow_run_id(&run);
                        let result = executor
                            .execute_run(&workflow_name, inputs, working_dir, run)
                            .await?;

                        match result.status {
                            crate::workflows::WorkflowStatus::Completed => {
                                println!(
                                    "\n   {} Workflow completed successfully in {}ms",
                                    Glyphs::flower(),
                                    result.duration_ms
                                );
                            }
                            crate::workflows::WorkflowStatus::Failed => {
                                println!(
                                    "\n   {} Workflow failed after {}ms",
                                    Glyphs::fallen_leaf(),
                                    result.duration_ms
                                );
                                if let Some(reason) = &result.stop_reason {
                                    println!("   {} Stopped: {}", Glyphs::fallen_leaf(), reason);
                                }
                                print_workflow_resume_hint(&result, &file);
                            }
                            other => {
                                println!(
                                    "\n   {} Workflow ended with status: {:?}",
                                    Glyphs::leaf(),
                                    other
                                );
                            }
                        }

                        print_workflow_telemetry(&result);
                        if !result.outputs.is_empty() {
                            println!("\n   {} Outputs:", Glyphs::journal());
                            for (name, value) in &result.outputs {
                                println!("      {} = {:?}", name.as_str().emphasis(), value);
                            }
                        }
                        println!();

                        // A workflow whose required steps failed must
                        // not exit zero (honest status, AGENTS.md §3).
                        if matches!(result.status, crate::workflows::WorkflowStatus::Failed) {
                            anyhow::bail!(
                                "workflow '{}' failed after {}ms{}",
                                workflow_name,
                                result.duration_ms,
                                result
                                    .stop_reason
                                    .as_ref()
                                    .map(|reason| format!(" — stopped: {reason}"))
                                    .unwrap_or_default()
                            );
                        }
                    }
                }
                Some(WorkflowFileKind::Yaml) => {
                    let source = super::subcommands::read_bounded_cli_file(
                        path,
                        4 * 1024 * 1024,
                        "YAML workflow file",
                    )?;
                    let workflow_name = serde_yaml::from_str::<crate::workflows::Workflow>(&source)
                        .map(|workflow| workflow.name)
                        .unwrap_or_else(|_| default_workflow_name(path));

                    let mut executor = if dry_run {
                        println!(
                            "\n{} {} (dry-run mode)\n",
                            Glyphs::gear(),
                            "YAML Workflow".workshop_title()
                        );
                        WorkflowExecutor::new_dry_run_with_config(&config.safety)
                    } else {
                        println!(
                            "\n{} {}\n",
                            Glyphs::gear(),
                            "YAML Workflow".workshop_title()
                        );
                        WorkflowExecutor::new_with_config(&config.safety)
                    };

                    load_related_yaml_workflows(&mut executor, path)?;

                    if !dry_run {
                        let client = std::sync::Arc::new(crate::api::ApiClient::new(config)?);
                        executor = executor.with_async_llm_handler(
                            build_workflow_llm_handler_with_safety(
                                client,
                                config.model.clone(),
                                &config.safety,
                            ),
                        );
                        executor =
                            executor.with_tool_handler(build_workflow_tool_handler(&config.safety));
                    }

                    println!(
                        "   {} File: {}",
                        Glyphs::journal(),
                        file.as_str().path_local()
                    );
                    println!(
                        "   {} Running workflow: {}",
                        Glyphs::compass(),
                        workflow_name.clone().emphasis()
                    );
                    if !inputs.is_empty() {
                        println!("   {} Inputs: {:?}", Glyphs::journal(), inputs);
                    }
                    println!();

                    let working_dir = std::env::current_dir()?;
                    let result = if dry_run {
                        executor
                            .execute(&workflow_name, inputs, working_dir)
                            .await?
                    } else {
                        let run = resumed_run.clone().unwrap_or_else(|| {
                            crate::workflows::WorkflowRun::fresh(
                                checkpoint_store.clone(),
                                &workflow_name,
                            )
                        });
                        print_workflow_run_id(&run);
                        executor
                            .execute_run(&workflow_name, inputs, working_dir, run)
                            .await?
                    };

                    match result.status {
                        crate::workflows::WorkflowStatus::Completed => {
                            println!(
                                "\n   {} Workflow completed successfully in {}ms",
                                Glyphs::flower(),
                                result.duration_ms
                            );
                        }
                        crate::workflows::WorkflowStatus::Failed => {
                            println!(
                                "\n   {} Workflow failed after {}ms",
                                Glyphs::fallen_leaf(),
                                result.duration_ms
                            );
                            if let Some(reason) = &result.stop_reason {
                                println!("   {} Stopped: {}", Glyphs::fallen_leaf(), reason);
                            }
                            print_workflow_resume_hint(&result, &file);
                        }
                        other => {
                            println!(
                                "\n   {} Workflow ended with status: {:?}",
                                Glyphs::leaf(),
                                other
                            );
                        }
                    }

                    print_workflow_telemetry(&result);
                    if !result.outputs.is_empty() {
                        println!("\n   {} Outputs:", Glyphs::journal());
                        for (name, value) in &result.outputs {
                            println!("      {} = {:?}", name.as_str().emphasis(), value);
                        }
                    }
                    println!();

                    // Same contract as the SWL path: a failed
                    // workflow must not exit zero (AGENTS.md §3).
                    if result.status == crate::workflows::WorkflowStatus::Failed {
                        anyhow::bail!(
                            "workflow '{}' failed after {}ms{}",
                            workflow_name,
                            result.duration_ms,
                            result
                                .stop_reason
                                .as_ref()
                                .map(|reason| format!(" — stopped: {reason}"))
                                .unwrap_or_default()
                        );
                    }
                }
                None => anyhow::bail!(
                    "Unsupported workflow file '{}'. Use .swl, .yaml, or .yml",
                    file
                ),
            }
        }

        WorkflowCommands::List { dir, all } => {
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "Available Workflows".workshop_title()
            );

            let dir_path = std::path::Path::new(&dir);
            if !dir_path.exists() {
                anyhow::bail!("Directory not found: {}", dir);
            }

            // Find SWL files
            let mut swl_files = Vec::new();
            let mut yaml_files = Vec::new();

            for entry in walkdir::WalkDir::new(dir_path).max_depth(2) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "swl" {
                            swl_files.push(path.to_path_buf());
                        } else if all && (ext == "yaml" || ext == "yml") {
                            // Try to parse as workflow file
                            if let Ok(content) = super::subcommands::read_bounded_cli_file(
                                path,
                                4 * 1024 * 1024,
                                "YAML workflow file",
                            ) {
                                if content.contains("workflows:") || content.contains("steps:") {
                                    yaml_files.push(path.to_path_buf());
                                }
                            }
                        }
                    }
                }
            }

            if swl_files.is_empty() && yaml_files.is_empty() {
                println!("   No workflow files found.\n");
                println!(
                    "   {} Tip: Create .swl files to define workflows\n",
                    Glyphs::sprout()
                );
            } else {
                if !swl_files.is_empty() {
                    println!("   {} SWL Workflows:", Glyphs::journal());
                    for file in &swl_files {
                        let name = file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown");
                        let rel_path = file.strip_prefix(dir_path).unwrap_or(file.as_path());
                        println!(
                            "      {} {} ({})",
                            Glyphs::flower(),
                            name.to_string().emphasis(),
                            rel_path.display()
                        );
                    }
                    println!();
                }

                if all && !yaml_files.is_empty() {
                    println!("   {} YAML Workflows:", Glyphs::journal());
                    for file in &yaml_files {
                        let name = file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown");
                        let rel_path = file.strip_prefix(dir_path).unwrap_or(file.as_path());
                        println!("      {} {} ({})", Glyphs::leaf(), name, rel_path.display());
                    }
                    println!();
                }

                println!(
                    "   Found {} workflow file(s)\n",
                    (swl_files.len() + yaml_files.len())
                        .to_string()
                        .garden_healthy()
                );
            }
        }
    }

    Ok(())
}
