# SWL Runtime Documentation

Selfware Workflow Language (SWL) is a declarative-imperative hybrid language for agent orchestration with first-class observability.

## Table of Contents

- [Overview](#overview)
- [Creating Workflows](#creating-workflows)
- [Agent Definition Format](#agent-definition-format)
- [Workflow Types](#workflow-types)
- [Validating Workflows](#validating-workflows)
- [Running Workflows](#running-workflows)
- [Examples](#examples)

## Overview

SWL treats agent orchestration as **infrastructure-as-code** with **first-class observability**. Every construct emits telemetry automatically.

```yaml
# example.swl - Minimal workflow
version: "1.0"
name: hello_world

agents:
  greeter:
    model: claude-sonnet-4
    role: A friendly greeting agent
    instruction: Generate a warm welcome message

workflows:
  say_hello:
    type: sequential
    steps:
      - delegate: greeter
```

## Creating Workflows

### File Structure

SWL files use the `.swl` extension and are written in YAML format:

```yaml
version: "1.0"                    # SWL version
name: my_workflow                 # Workflow name
description: Optional description

agents:
  # Agent definitions

workflows:
  # Workflow definitions

guardrails:
  # Safety guardrails

telemetry:
  # Observability configuration

state:
  # State schema definition
```

### Basic Components

| Component | Description | Required |
|-----------|-------------|----------|
| `version` | SWL version string | Yes |
| `name` | Workflow identifier | Yes |
| `agents` | Map of agent definitions | Yes |
| `workflows` | Map of workflow definitions | Yes |
| `guardrails` | List of safety constraints | No |
| `telemetry` | Metrics and tracing config | No |
| `state` | State schema definition | No |

## Agent Definition Format

### Simple Agent

```yaml
agents:
  my_agent:
    model: claude-sonnet-4
    role: Brief role description
    instruction: What the agent should do
```

### Detailed Agent Configuration

```yaml
agents:
  code_reviewer:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.1
      max_tokens: 8192
    role: |
      You are a staff engineer doing code review.
      You catch bugs that pass CI.
    instruction: |
      Review the code diff thoroughly.
      Check for: logic errors, edge cases, security issues.
    tools:
      - file_read
      - file_diff
      - git_log
    output_key: review_output
    sub_agents: []
```

### Agent Fields

| Field | Type | Description | Required |
|-------|------|-------------|----------|
| `model` | string or object | Model specification | Yes |
| `role` | string | Agent's persona/identity | No |
| `instruction` | string | Task instructions | No |
| `tools` | list | Available tool names | No |
| `output_key` | string | Key for storing output | No |
| `sub_agents` | list | Nested agent names | No |

### Model Specification

**Simple format:**
```yaml
model: claude-sonnet-4
```

**Detailed format:**
```yaml
model:
  provider: openai              # API provider
  name: model-name              # Model identifier
  temperature: 0.1              # Sampling temperature (0-1)
  max_tokens: 8192              # Maximum output tokens
```

## Workflow Types

### Sequential

Execute agents one after another:

```yaml
workflows:
  pipeline:
    type: sequential
    description: "Step-by-step processing"
    steps:
      - delegate: data_loader
      - delegate: processor
        input:
          mode: "strict"
      - delegate: validator
```

### Parallel

Execute agents concurrently:

```yaml
workflows:
  multi_review:
    type: parallel
    description: "Run all reviewers at once"
    steps:
      - delegate: security_reviewer
      - delegate: performance_reviewer
      - delegate: test_reviewer
    
    # Optional merge phase
    merge:
      agent: aggregator
      instruction: "Consolidate all reviews"
```

### Map-Reduce

Distribute work across agents, then aggregate:

```yaml
workflows:
  distributed_analysis:
    type: map_reduce
    description: "Parallel analysis with aggregation"
    
    map:
      targets: [backend_specialist, frontend_specialist, devops_specialist]
      input:
        scope: "full"
      parallel:
        branches:
          - delegate: backend_specialist
          - delegate: frontend_specialist
          - delegate: devops_specialist
    
    reduce:
      agent: consensus_aggregator
      inputs:
        - backend_analysis
        - frontend_analysis
        - devops_analysis
```

### Conditional

Branch based on conditions:

```yaml
workflows:
  smart_pipeline:
    type: conditional
    steps:
      - delegate: condition_checker
      - guard:
          condition:
            language: rust
            content: |
              args.previous_output.contains("proceed")
          on_violation: block
      - delegate: next_step
```

### Workflow Step Types

| Step Type | Description |
|-----------|-------------|
| `delegate` | Execute an agent |
| `parallel` | Execute branches concurrently |
| `guard` | Conditional execution with guardrails |


## Validating Workflows

Validate SWL files before running:

```bash
# Validate an SWL file
selfware workflow validate workflows/my_workflow.swl

# Validate with detailed output
selfware workflow validate workflows/product_build.yml
```

Validation checks:
- Required fields (version, name, agents, workflows)
- Agent references exist
- Model specifications are valid
- Workflow type compatibility
- Guardrail conditions
- State schema definitions

### Validation Example Output

```
⚙ Workflow Validation

   📝 File: workflows/code_review.swl

   ✓ SWL file is valid!
   Name: comprehensive_code_review
   Version: 1.0
   Agents: 4
   Workflows: 3
```

## Running Workflows

### Dry-Run Mode

Test workflows without executing LLM calls:

```bash
# Dry-run to verify workflow structure
selfware workflow run workflows/product_build.yml --dry-run \
  --input idea="Daily briefing app" \
  --input target_user="Busy founders"
```

Dry-run mode:
- Parses and validates the workflow
- Logs intended actions
- Does not make API calls
- Useful for testing and debugging

### Live Execution

Run workflows with actual LLM calls:

```bash
# Run with inputs
selfware workflow run workflows/product_build.yml \
  --input idea="Daily briefing app for founders" \
  --input target_user="Busy founders" \
  --input constraints="Rust backend, web frontend" \
  --input verification_command="cargo test --quiet"

# Run without inputs (uses defaults)
selfware workflow run workflows/code_review.swl
```

### Listing Workflows

```bash
# List SWL workflows in current directory
selfware workflow list

# List all workflows including YAML
selfware workflow list --all

# List workflows in specific directory
selfware workflow list --dir ./workflows
```


## Examples

### Example 1: Code Review Workflow

```yaml
# code_review.swl
version: "1.0"
name: comprehensive_code_review
description: Multi-layer code review

agents:
  staff_engineer:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.1
    role: Staff engineer reviewing code
    instruction: Review for logic errors, edge cases, and design
    tools:
      - file_read
      - file_diff
    output_key: staff_review

  security_reviewer:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.1
    role: Security engineer
    instruction: Focus on OWASP Top 10 and security issues
    tools:
      - file_read
    output_key: security_review

workflows:
  full_review:
    type: parallel
    steps:
      - delegate: staff_engineer
      - delegate: security_reviewer
    merge:
      agent: staff_engineer
      instruction: Consolidate all findings

guardrails:
  - name: block_if_critical
    type: post_agent
    condition:
      language: rust
      content: |
        !args.agent_outputs.security_review.contains("[CRITICAL]")
    on_violation: block
```

### Example 2: Multi-Agent Swarm

```yaml
# multi_agent_swarm.swl
version: "1.0"
name: multi_agent_swarm
description: Parallel specialists with consensus

agents:
  backend_specialist:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.3
    role: Backend architecture specialist
    instruction: Analyze from backend perspective
    output_key: backend_analysis

  frontend_specialist:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.3
    role: Frontend architecture specialist
    instruction: Analyze from frontend perspective
    output_key: frontend_analysis

  consensus_aggregator:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.2
    role: Synthesize expert opinions
    instruction: Create integrated implementation plan
    output_key: consensus_spec

workflows:
  swarm_with_consensus:
    type: map_reduce
    map:
      parallel:
        branches:
          - delegate: backend_specialist
          - delegate: frontend_specialist
    reduce:
      agent: consensus_aggregator
      inputs:
        - backend_analysis
        - frontend_analysis
```

### Example 3: Product Build Workflow

```yaml
# product_build.yml (YAML workflow format)
name: product_build
description: End-to-end product build workflow
version: "1.0.0"

inputs:
  - name: idea
    description: Product idea
    required: true
  - name: target_user
    description: Primary user persona
    required: false
    default: "Technical teams"

outputs:
  - name: discovery_brief
    description: Product discovery brief
    from: discovery_brief

steps:
  - id: kickoff
    name: Kickoff
    type: log
    message: "Building: ${idea}"
    level: info

  - id: discovery_brief
    name: Discovery
    type: llm
    prompt: |
      Analyze this product idea: ${idea}
      Target user: ${target_user}
      Create a discovery brief.
    timeout_secs: 180
```

### Example 4: Simple Test Workflow

```yaml
# test_simple.swl
version: "1.0"
name: test_workflow

agents:
  test_agent:
    model: claude-sonnet-4
    role: tester
    instruction: Run tests

workflows:
  test_flow:
    type: sequential

guardrails:
  - name: test_guard
    type: block
    condition: "true"
    on_violation: warn

telemetry:
  traces:
    - inference
```

## Guardrails

Guardrails provide safety constraints:

```yaml
guardrails:
  - name: rate_limiter
    type: before_model
    condition: |
      rate(requests_per_minute) < 60
    on_violation: queue_and_retry
    
  - name: cost_cap
    type: global
    condition:
      language: rust
      content: |
        total_cost < budget.limit
    on_violation: pause_and_notify
```

### Guardrail Types

| Type | Description |
|------|-------------|
| `before_model` | Check before LLM call |
| `before_tool` | Check before tool execution |
| `post_agent` | Check after agent completes |
| `post_workflow` | Check after workflow completes |
| `global` | Always active |

### Violation Actions

| Action | Description |
|--------|-------------|
| `block` | Stop execution |
| `warn` | Log warning and continue |
| `queue_and_retry` | Queue for later retry |
| `pause_and_notify` | Pause and notify user |

## Telemetry

Configure observability:

```yaml
telemetry:
  enabled: true
  metrics:
    - parallel_agent_count
    - consensus_time_ms
    - agreement_percentage
  traces:
    - workflow_execution
    - agent_delegation
    - tool_invocation
  export:
    type: file
    path: "./logs/telemetry.jsonl"
```

## State Management

Define workflow state schema:

```yaml
state:
  fields:
    - name: task_description
      type: string
      description: The task being analyzed
    
    - name: specialist_count
      type: integer
      description: Number of specialists
      default: 4
    
    - name: consensus_reached
      type: boolean
      description: Whether consensus was achieved
      default: false
    
    - name: conflict_areas
      type: array
      element_type: string
      description: Areas of disagreement
      default: []
```

## CLI Commands Reference

| Command | Description |
|---------|-------------|
| `selfware workflow validate <file>` | Validate workflow file |
| `selfware workflow run <file>` | Execute workflow |
| `selfware workflow run <file> --dry-run` | Test without execution |
| `selfware workflow list` | List available workflows |
| `selfware workflow list --all` | List all formats |

## File Formats

SWL supports two workflow formats:

| Format | Extension | Use Case |
|--------|-----------|----------|
| SWL | `.swl` | Agent orchestration |
| YAML | `.yml`, `.yaml` | Step-based workflows |

## Further Reading

- See example workflows in this directory


---

## Product Workflow Set

The product workflow set is inspired by `garrytan/gstack` and `alirezarezvani/claude-skills`.

### Files

- `product_discovery.yml`: Discovery brief plus assumption risk register
- `product_delivery.yml`: Architecture, sprint plan, and review gate
- `product_release.yml`: Release checklist, docs plan, and launch note
- `product_build.yml`: Orchestrates the three workflows above

### Product Workflow Example

```bash
# Validate the product workflow
selfware workflow validate workflows/product_build.yml

# Run in dry-run mode
selfware workflow run workflows/product_build.yml \
  --input idea="Daily briefing app for founders" \
  --input target_user="Busy founders" \
  --input constraints="Rust backend, web frontend, ship in one week" \
  --input repo_context="Selfware repo with workflow and agent runtime" \
  --input verification_command="cargo test --quiet" \
  --input smoke_test_command="cargo test --quiet" \
  --dry-run
```

For a configuration starting point, see the checked-in `selfware.example.toml`.
