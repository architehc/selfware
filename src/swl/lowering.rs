use crate::errors::{Result, SelfwareError};
use crate::swl::parser::ast::{
    AgentDefinition, AggregateStage, GuardCondition, Guardrail, ParallelStage, ReduceStage,
    SwlDocument, WorkflowDefinition, WorkflowStep as SwlWorkflowStep, WorkflowType,
};
use crate::workflows::{
    LogLevel, RetryConfig, StepType, VarValue, Workflow, WorkflowInput, WorkflowOutput,
    WorkflowStep,
};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct LoweredSwl {
    pub workflows: Vec<Workflow>,
    pub warnings: Vec<String>,
}

pub fn lower_document(doc: &SwlDocument) -> Result<LoweredSwl> {
    let mut lowered = Vec::new();
    let mut warnings = Vec::new();

    for (name, workflow) in &doc.workflows {
        let mut ctx = LoweringContext::new(doc, name, workflow);
        lowered.push(ctx.lower_workflow()?);
        warnings.extend(ctx.warnings);
    }

    Ok(LoweredSwl {
        workflows: lowered,
        warnings,
    })
}

struct LoweringContext<'a> {
    doc: &'a SwlDocument,
    workflow_name: &'a str,
    workflow: &'a WorkflowDefinition,
    step_counter: usize,
    inputs: BTreeMap<String, WorkflowInput>,
    outputs: BTreeSet<String>,
    warnings: Vec<String>,
}

impl<'a> LoweringContext<'a> {
    fn new(doc: &'a SwlDocument, workflow_name: &'a str, workflow: &'a WorkflowDefinition) -> Self {
        let mut inputs = BTreeMap::new();

        if let Some(state) = &doc.state {
            for field in &state.fields {
                inputs.insert(
                    field.name.clone(),
                    WorkflowInput {
                        name: field.name.clone(),
                        description: field.description.clone().unwrap_or_default(),
                        required: false,
                        default: field.default.clone().map(yaml_to_var_value),
                        param_type: field.field_type.type_name().to_string(),
                    },
                );
            }
        }

        Self {
            doc,
            workflow_name,
            workflow,
            step_counter: 0,
            inputs,
            outputs: BTreeSet::new(),
            warnings: Vec::new(),
        }
    }

    fn lower_workflow(&mut self) -> Result<Workflow> {
        let tags = self
            .doc
            .metadata
            .as_ref()
            .map(|m| m.tags.clone())
            .unwrap_or_default();
        let author = self
            .doc
            .metadata
            .as_ref()
            .and_then(|m| m.author.clone())
            .unwrap_or_default();

        let mut steps = Vec::new();
        let mut available = initial_available_vars(self.doc);

        let exit_ids = match self.workflow.workflow_type {
            WorkflowType::Sequential => {
                self.lower_sequential_steps(&self.workflow.steps, &[], &mut available, &mut steps)?
            }
            WorkflowType::Parallel => {
                self.lower_parallel_steps(&self.workflow.steps, &[], &mut available, &mut steps)?
            }
            WorkflowType::MapReduce => self.lower_map_reduce(&mut available, &mut steps)?,
            WorkflowType::Conditional => {
                // Conditional workflows are sequential with guard steps that can
                // block downstream execution.  We reuse lower_sequential_steps
                // which already handles per-step guards via lower_step / lower_guard_warning.
                self.lower_sequential_steps(&self.workflow.steps, &[], &mut available, &mut steps)?
            }
        };

        let mut current_exits = exit_ids;

        if let Some(merge) = &self.workflow.merge {
            current_exits =
                self.lower_aggregate_stage(merge, current_exits, &available, &mut steps, "merge")?;
            self.record_agent_output(&merge.agent, &mut available);
        }

        let _final_exits = if !self.doc.guardrails.is_empty() {
            self.lower_guardrail_warnings(&self.doc.guardrails, current_exits, &mut steps)
        } else {
            current_exits
        };

        let outputs = self
            .outputs
            .iter()
            .map(|name| WorkflowOutput {
                name: name.clone(),
                description: format!("Lowered SWL output '{}'", name),
                from: name.clone(),
            })
            .collect();

        Ok(Workflow {
            name: self.workflow_name.to_string(),
            description: self
                .workflow
                .description
                .clone()
                .or_else(|| self.doc.description.clone())
                .unwrap_or_default(),
            version: self.doc.version.clone(),
            author,
            category: "swl".to_string(),
            inputs: self.inputs.values().cloned().collect(),
            outputs,
            steps,
            tags,
        })
    }

    fn lower_map_reduce(
        &mut self,
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        let map = self.workflow.map.as_ref().ok_or_else(|| {
            SelfwareError::Internal(format!(
                "SWL workflow '{}' is missing map stage",
                self.workflow_name
            ))
        })?;

        let exits = if let Some(parallel) = &map.parallel {
            self.lower_parallel_branches(parallel, &[], available, steps)?
        } else {
            let synthetic_steps = map
                .targets
                .iter()
                .map(|target| SwlWorkflowStep {
                    name: None,
                    delegate: Some(target.clone()),
                    agent: None,
                    action: None,
                    input: map.input.clone(),
                    parallel: None,
                    guard: None,
                })
                .collect::<Vec<_>>();

            self.lower_parallel_steps(&synthetic_steps, &[], available, steps)?
        };

        if let Some(ReduceStage::Aggregate(stage)) = &self.workflow.reduce {
            let reduce_exits =
                self.lower_aggregate_stage(stage, exits, available, steps, "reduce")?;
            self.record_agent_output(&stage.agent, available);
            Ok(reduce_exits)
        } else {
            Ok(exits)
        }
    }

    fn lower_sequential_steps(
        &mut self,
        swl_steps: &[SwlWorkflowStep],
        incoming: &[String],
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        let mut current = incoming.to_vec();

        for step in swl_steps {
            current = self.lower_step(step, &current, available, steps)?;
        }

        Ok(current)
    }

    fn lower_parallel_steps(
        &mut self,
        swl_steps: &[SwlWorkflowStep],
        incoming: &[String],
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        let mut exits = Vec::new();
        let base_available = available.clone();
        let mut new_outputs = Vec::new();

        for step in swl_steps {
            let mut branch_available = base_available.clone();
            let branch_exits = self.lower_step(step, incoming, &mut branch_available, steps)?;
            exits.extend(branch_exits);

            for var in branch_available {
                if !base_available.contains(&var) && !new_outputs.contains(&var) {
                    new_outputs.push(var);
                }
            }
        }

        available.extend(new_outputs);
        dedupe_strings(available);
        Ok(exits)
    }

    fn lower_parallel_branches(
        &mut self,
        parallel: &ParallelStage,
        incoming: &[String],
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        self.lower_parallel_steps(&parallel.branches, incoming, available, steps)
    }

    fn lower_step(
        &mut self,
        step: &SwlWorkflowStep,
        incoming: &[String],
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        if let Some(delegate) = &step.delegate {
            return self.lower_delegate_step(delegate, step, incoming, available, steps);
        }

        if let Some(agent) = &step.agent {
            return self.lower_legacy_agent_step(agent, step, incoming, available, steps);
        }

        if let Some(parallel) = &step.parallel {
            return self.lower_parallel_branches(parallel, incoming, available, steps);
        }

        if let Some(guard) = &step.guard {
            return Ok(self.lower_guard_warning(
                &format!(
                    "step guard ({})",
                    step.name.clone().unwrap_or_else(|| "unnamed".to_string())
                ),
                &guard.condition,
                &guard.on_violation,
                incoming.to_vec(),
                steps,
            ));
        }

        Err(SelfwareError::Internal(format!(
            "SWL step in workflow '{}' had no supported lowering shape",
            self.workflow_name
        )))
    }

    fn lower_delegate_step(
        &mut self,
        agent_name: &str,
        step: &SwlWorkflowStep,
        incoming: &[String],
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        let agent = self.doc.agents.get(agent_name).ok_or_else(|| {
            SelfwareError::Internal(format!(
                "SWL workflow '{}' references unknown agent '{}'",
                self.workflow_name, agent_name
            ))
        })?;

        let step_id = self.next_step_id(
            step.name
                .as_deref()
                .or(step.delegate.as_deref())
                .unwrap_or(agent_name),
        );
        let llm_step = WorkflowStep {
            id: step_id.clone(),
            name: step
                .name
                .clone()
                .unwrap_or_else(|| format!("Run {}", agent_name)),
            description: format!("Lowered SWL delegate step for '{}'", agent_name),
            step_type: StepType::Llm {
                prompt: build_agent_prompt(agent, agent_name, &step.input),
                context: build_context_entries(&step.input, available, self, agent_name)?,
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: Some(240),
            depends_on: incoming.to_vec(),
        };
        steps.push(llm_step);

        let mut exits = vec![step_id.clone()];

        // Always make the step_id available as an output so parallel workflows can capture results
        if !available.contains(&step_id) {
            available.push(step_id.clone());
        }
        self.outputs.insert(step_id.clone());

        if let Some(output_key) = agent.output_key.as_deref() {
            exits = self.capture_output(output_key, &step_id, steps);
            if !available.contains(&output_key.to_string()) {
                available.push(output_key.to_string());
            }
            self.outputs.insert(output_key.to_string());
        }

        Ok(exits)
    }

    fn lower_legacy_agent_step(
        &mut self,
        agent_name: &str,
        step: &SwlWorkflowStep,
        incoming: &[String],
        available: &mut Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Result<Vec<String>> {
        if step.action.as_deref() == Some("echo") {
            let step_id = self.next_step_id(step.name.as_deref().unwrap_or(agent_name));
            let message = step
                .input
                .as_ref()
                .map(|value| normalize_value(value, available, self))
                .transpose()?
                .unwrap_or_default();

            steps.push(WorkflowStep {
                id: step_id.clone(),
                name: step
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Echo {}", agent_name)),
                description: format!("Lowered legacy SWL action for '{}'", agent_name),
                step_type: StepType::Log {
                    message,
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: Some(30),
                depends_on: incoming.to_vec(),
            });

            return Ok(vec![step_id]);
        }

        self.lower_delegate_step(
            step.agent.as_deref().unwrap_or(agent_name),
            &SwlWorkflowStep {
                name: step.name.clone(),
                delegate: Some(agent_name.to_string()),
                agent: None,
                action: None,
                input: step.input.clone(),
                parallel: None,
                guard: None,
            },
            incoming,
            available,
            steps,
        )
    }

    fn lower_aggregate_stage(
        &mut self,
        stage: &AggregateStage,
        incoming: Vec<String>,
        available: &[String],
        steps: &mut Vec<WorkflowStep>,
        stage_kind: &str,
    ) -> Result<Vec<String>> {
        let agent = self.doc.agents.get(&stage.agent).ok_or_else(|| {
            SelfwareError::Internal(format!(
                "SWL workflow '{}' references unknown aggregate agent '{}'",
                self.workflow_name, stage.agent
            ))
        })?;

        let stage_id = self.next_step_id(&format!("{}_{}", stage_kind, stage.agent));
        let mut context = stage
            .inputs
            .iter()
            .map(|input| normalize_string(input, available, self))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if context.is_empty() {
            context = available
                .iter()
                .map(|name| format!("${{{}}}", name))
                .collect();
        }

        let prompt = if let Some(instruction) = &stage.instruction {
            instruction.clone()
        } else {
            build_agent_prompt(agent, &stage.agent, &None)
        };

        steps.push(WorkflowStep {
            id: stage_id.clone(),
            name: format!("{} {}", capitalize(stage_kind), stage.agent),
            description: format!("Lowered SWL {} stage", stage_kind),
            step_type: StepType::Llm { prompt, context },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: Some(240),
            depends_on: incoming,
        });

        let mut exits = vec![stage_id.clone()];
        if let Some(output_key) = agent.output_key.as_deref() {
            exits = self.capture_output(output_key, &stage_id, steps);
            self.outputs.insert(output_key.to_string());
        }

        Ok(exits)
    }

    fn capture_output(
        &mut self,
        output_key: &str,
        source_step_id: &str,
        steps: &mut Vec<WorkflowStep>,
    ) -> Vec<String> {
        let capture_id = self.next_step_id(&format!("capture_{}", output_key));
        steps.push(WorkflowStep {
            id: capture_id.clone(),
            name: format!("Capture {}", output_key),
            description: format!("Bind SWL output '{}' to workflow variables", output_key),
            step_type: StepType::SetVar {
                name: output_key.to_string(),
                value: format!("${{{}}}", source_step_id),
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: Some(30),
            depends_on: vec![source_step_id.to_string()],
        });
        vec![capture_id]
    }

    fn record_agent_output(&mut self, agent_name: &str, available: &mut Vec<String>) {
        if let Some(output_key) = self
            .doc
            .agents
            .get(agent_name)
            .and_then(|agent| agent.output_key.clone())
        {
            if !available.contains(&output_key) {
                available.push(output_key.clone());
            }
            self.outputs.insert(output_key);
        }
    }

    fn lower_guardrail_warnings(
        &mut self,
        guardrails: &[Guardrail],
        incoming: Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Vec<String> {
        let mut current = incoming;
        for guardrail in guardrails {
            let name = guardrail
                .name
                .clone()
                .unwrap_or_else(|| "unnamed_guardrail".to_string());
            current = self.lower_guard_warning(
                &name,
                &guardrail.condition,
                &guardrail.on_violation,
                current,
                steps,
            );
        }
        current
    }

    fn lower_guard_warning(
        &mut self,
        name: &str,
        condition: &GuardCondition,
        on_violation: &str,
        incoming: Vec<String>,
        steps: &mut Vec<WorkflowStep>,
    ) -> Vec<String> {
        let condition_summary = match condition {
            GuardCondition::Inline(text) => text.clone(),
            GuardCondition::Code(block) => format!("{} block", block.language.as_str()),
        };
        self.warnings.push(format!(
            "SWL workflow '{}' contains unenforced guard '{}': {} ({})",
            self.workflow_name, name, on_violation, condition_summary
        ));

        let step_id = self.next_step_id(&format!("guard_{}", name));
        steps.push(WorkflowStep {
            id: step_id.clone(),
            name: format!("Guard {}", name),
            description: "Lowered SWL guard warning".to_string(),
            step_type: StepType::Log {
                message: format!(
                    "[SWL lowering] Guard '{}' with on_violation='{}' is not enforced by WorkflowExecutor yet",
                    name, on_violation
                ),
                level: LogLevel::Warn,
            },
            required: false,
            retry: RetryConfig::default(),
            timeout_secs: Some(30),
            depends_on: incoming,
        });
        vec![step_id]
    }

    fn next_step_id(&mut self, hint: &str) -> String {
        self.step_counter += 1;
        format!("{:03}_{}", self.step_counter, slugify(hint))
    }
}

fn build_agent_prompt(
    agent: &AgentDefinition,
    agent_name: &str,
    input: &Option<serde_yaml::Value>,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!("SWL agent: {}", agent_name));
    sections.push(format!("Model: {}", agent.model.name()));

    if let Some(role) = &agent.role {
        sections.push(format!("Role:\n{}", role.trim()));
    }
    if let Some(instruction) = &agent.instruction {
        sections.push(format!("Instruction:\n{}", instruction.trim()));
    }
    if !agent.tools.is_empty() {
        sections.push(format!("Requested tools: {}", agent.tools.join(", ")));
    }
    if input.is_some() {
        sections.push("Execute the task using the workflow inputs provided in context. Return only the task result.".to_string());
    } else {
        sections.push("Execute the task and return only the task result.".to_string());
    }

    sections.join("\n\n")
}

fn build_context_entries(
    input: &Option<serde_yaml::Value>,
    available: &[String],
    ctx: &mut LoweringContext<'_>,
    agent_name: &str,
) -> Result<Vec<String>> {
    let mut entries = Vec::new();

    if let Some(value) = input {
        match value {
            serde_yaml::Value::Mapping(map) => {
                for (key, value) in map {
                    let key = yaml_scalar_to_key(key).unwrap_or_else(|| "input".to_string());
                    entries.push(format!(
                        "{}: {}",
                        key,
                        normalize_value(value, available, ctx)?
                    ));
                }
            }
            other => {
                entries.push(normalize_value(other, available, ctx)?);
            }
        }
    }

    if let Some(output_key) = ctx
        .doc
        .agents
        .get(agent_name)
        .and_then(|agent| agent.output_key.clone())
    {
        ctx.outputs.insert(output_key);
    }

    Ok(entries)
}

fn normalize_value(
    value: &serde_yaml::Value,
    available: &[String],
    ctx: &mut LoweringContext<'_>,
) -> Result<String> {
    match value {
        serde_yaml::Value::String(s) => normalize_string(s, available, ctx),
        serde_yaml::Value::Sequence(seq) => {
            let items = seq
                .iter()
                .map(|item| normalize_value(item, available, ctx))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(items.join(", "))
        }
        serde_yaml::Value::Mapping(map) => {
            let mut rendered = Vec::new();
            for (key, value) in map {
                let key = yaml_scalar_to_key(key).unwrap_or_else(|| "item".to_string());
                rendered.push(format!(
                    "{}: {}",
                    key,
                    normalize_value(value, available, ctx)?
                ));
            }
            Ok(rendered.join("\n"))
        }
        serde_yaml::Value::Bool(b) => Ok(b.to_string()),
        serde_yaml::Value::Number(n) => Ok(n.to_string()),
        serde_yaml::Value::Null => Ok(String::new()),
        _ => Ok(serde_yaml::to_string(value)
            .unwrap_or_default()
            .trim()
            .to_string()),
    }
}

fn normalize_string(
    raw: &str,
    available: &[String],
    ctx: &mut LoweringContext<'_>,
) -> Result<String> {
    let mut out = raw.to_string();
    let captures = jinja_regex()
        .captures_iter(raw)
        .map(|caps| {
            let full = caps
                .get(0)
                .map(|m| m.as_str().to_string())
                .ok_or_else(|| SelfwareError::Internal("missing full regex capture".to_string()))?;
            let expr = caps
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .ok_or_else(|| SelfwareError::Internal("missing expression capture".to_string()))?;
            Ok::<(String, String), SelfwareError>((full, expr))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    for (full, expr) in captures {
        let replacement = convert_expression(&expr, available, ctx)?;
        out = out.replace(&full, &replacement);
    }

    Ok(out)
}

fn convert_expression(
    expr: &str,
    available: &[String],
    ctx: &mut LoweringContext<'_>,
) -> Result<String> {
    if expr == "all_previous" {
        let joined = available
            .iter()
            .map(|name| format!("${{{}}}", name))
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok(joined);
    }

    if let Some((name, default)) = parse_default_expression(expr) {
        ensure_input(ctx, &name, Some(VarValue::String(default.clone())), false);
        return Ok(format!("${{{}}}", name));
    }

    if let Some(mapped) = parse_agent_output_reference(expr, ctx.doc) {
        return Ok(format!("${{{}}}", mapped));
    }

    if is_simple_identifier(expr) {
        ensure_input_if_external(ctx, expr, available);
        return Ok(format!("${{{}}}", expr));
    }

    ctx.warnings.push(format!(
        "SWL workflow '{}' uses expression '{{{{{}}}}}' that was lowered conservatively",
        ctx.workflow_name, expr
    ));
    Ok(expr.to_string())
}

fn parse_default_expression(expr: &str) -> Option<(String, String)> {
    let caps = default_regex().captures(expr)?;
    let name = caps.get(1)?.as_str().trim().to_string();
    let default = caps.get(2)?.as_str().to_string();
    Some((name, default))
}

fn parse_agent_output_reference(expr: &str, doc: &SwlDocument) -> Option<String> {
    let caps = agent_output_regex().captures(expr)?;
    let agent_name = caps.get(1)?.as_str();
    doc.agents
        .get(agent_name)
        .and_then(|agent| agent.output_key.clone())
}

fn ensure_input_if_external(ctx: &mut LoweringContext<'_>, name: &str, available: &[String]) {
    if available.contains(&name.to_string()) || ctx.inputs.contains_key(name) {
        return;
    }

    ensure_input(ctx, name, None, true);
}

fn ensure_input(
    ctx: &mut LoweringContext<'_>,
    name: &str,
    default: Option<VarValue>,
    required: bool,
) {
    ctx.inputs
        .entry(name.to_string())
        .and_modify(|input| {
            if input.default.is_none() {
                input.default = default.clone();
            }
            input.required = input.required || required;
        })
        .or_insert_with(|| WorkflowInput {
            name: name.to_string(),
            description: format!("Derived SWL input '{}'", name),
            required,
            default,
            param_type: "string".to_string(),
        });
}

fn initial_available_vars(doc: &SwlDocument) -> Vec<String> {
    let mut vars = doc
        .state
        .as_ref()
        .map(|state| {
            state
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    dedupe_strings(&mut vars);
    vars
}

fn yaml_to_var_value(value: serde_yaml::Value) -> VarValue {
    match value {
        serde_yaml::Value::String(s) => VarValue::String(s),
        serde_yaml::Value::Bool(b) => VarValue::Boolean(b),
        serde_yaml::Value::Number(n) => VarValue::Number(n.as_f64().unwrap_or_default()),
        serde_yaml::Value::Sequence(seq) => {
            VarValue::List(seq.into_iter().map(yaml_to_var_value).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut rendered = HashMap::new();
            for (key, value) in map {
                if let Some(key) = yaml_scalar_to_key(&key) {
                    rendered.insert(key, yaml_to_var_value(value));
                }
            }
            VarValue::Map(rendered)
        }
        _ => VarValue::Null,
    }
}

fn yaml_scalar_to_key(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }

    out.trim_matches('_').to_string()
}

fn capitalize(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn jinja_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\{\{\s*(.*?)\s*\}\}").expect("valid jinja regex"))
}

fn default_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^([A-Za-z0-9_]+)\s*\|\s*default\('([^']*)'\)$").expect("valid default regex")
    })
}

fn agent_output_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^([A-Za-z0-9_]+)\.output(?:\.[A-Za-z0-9_]+)*$")
            .expect("valid agent output regex")
    })
}

fn is_simple_identifier(expr: &str) -> bool {
    expr.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

impl crate::swl::parser::ast::CodeLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swl::parse_document;

    #[test]
    fn lower_code_review_produces_executor_workflow() {
        let source = std::fs::read_to_string("workflows/code_review.swl").unwrap();
        let doc = parse_document(&source).unwrap();
        let lowered = lower_document(&doc).unwrap();

        let workflow = lowered
            .workflows
            .iter()
            .find(|wf| wf.name == "full_review")
            .unwrap();
        assert!(!workflow.steps.is_empty());
        assert!(workflow.outputs.iter().any(|o| o.name == "staff_review"));
        assert!(lowered
            .warnings
            .iter()
            .any(|warning| warning.contains("guard")));
    }

    #[test]
    fn lower_multi_agent_swarm_emits_reduce_output() {
        let source = std::fs::read_to_string("workflows/multi_agent_swarm.swl").unwrap();
        let doc = parse_document(&source).unwrap();
        let lowered = lower_document(&doc).unwrap();

        let workflow = lowered
            .workflows
            .iter()
            .find(|wf| wf.name == "swarm_with_consensus")
            .unwrap();
        assert!(workflow.outputs.iter().any(|o| o.name == "consensus_spec"));
    }

    #[test]
    fn lower_legacy_test_execution_emits_log_steps() {
        let source = std::fs::read_to_string("workflows/test_execution.swl").unwrap();
        let doc = parse_document(&source).unwrap();
        let lowered = lower_document(&doc).unwrap();

        let workflow = lowered
            .workflows
            .iter()
            .find(|wf| wf.name == "main_flow")
            .unwrap();
        assert!(workflow
            .steps
            .iter()
            .any(|step| matches!(step.step_type, StepType::Log { .. })));
    }
}
