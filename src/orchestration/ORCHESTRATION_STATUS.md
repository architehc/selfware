# Orchestration Module Status

This document describes the current status of the orchestration module - what works and what's planned.

## Overview

The orchestration module provides multi-agent coordination capabilities for the selfware project. It includes several subsystems:

- **Multi-Agent Chat** (`multiagent/`): ⚠️ Functional beta — fixed 4-role, single-shot fan-out (no tool use)
- **Workflow Engine** (`workflows.rs`): ✅ Production Ready
- **Swarm Coordination** (`swarm/`): ✅ Functional (in-memory)
- **Coordinator Mode** (`swarm/coordinator.rs` + `multiagent/interactive.rs`): ⚠️ Functional beta — swarm task assignment gating `MultiAgentChat` execution

---

## ✅ Working: Multi-Agent Chat

**Location**: `src/orchestration/multiagent/`

**Status**: Functional single-shot fan-out. It works, but is narrower than the name suggests: each role agent makes exactly **one non-streaming chat completion** per task — **no tool use, no ReAct loop, no codebase context** — and the responses are aggregated for display. There is no cross-agent voting or synthesis step.

**What it actually does**:
- Spawns a fixed starting fleet of 4 role agents — Architect, Coder, Tester, Reviewer — each seeded with a role-specific system prompt (`MultiAgentConfig::default()`). More roles (documenter, devops, security, performance, general) can be added mid-session with `/add <role>`.
- Runs one non-streaming chat completion per agent per task: `ApiClient::chat(messages, None, …)` — the `None` means no tools are offered to the model.
- Limits concurrency with a semaphore. `-n`/`--concurrency` (default 4, clamped to 1–16) caps how many agents run at once; it does **not** change the fleet size.
- Failure policies (BestEffort by default, FailFast optional), per-agent timeout, result aggregation.
- Reports per-run token and cost totals after each task.
- Budget guardrails: `--max-budget-tokens` and `--max-cost-usd` are enforced across the fan-out. `--max-turns` is not applicable here — there is no agent loop to bound.

**Interactive commands** (`selfware multi-chat`):
`/agents`, `/status`, `/parallel N`, `/add <role>`, `/remove N`, `/clear`, `exit`

**Usage**:
```rust
use selfware::orchestration::multiagent::{MultiAgentChat, MultiAgentConfig};

let config = MultiAgentConfig::default();
let chat = MultiAgentChat::new(&api_config, config)?;
let results = chat.run_task("Review this code").await?;
```

---

## ✅ Working: Workflow Engine

**Location**: `src/orchestration/workflows.rs`

**Status**: Production ready with shell execution, tool integration, and LLM steps.

**Features**:
- YAML-defined workflows
- Step types: Shell, Tool, LLM, Condition, Loop, SetVar, Log, Pause, SubWorkflow
- Variable substitution with shell-safe quoting
- Retry logic with exponential backoff
- Dependency management between steps
- Dry-run mode for testing
- Guardrail integration

**Usage**:
```rust
use selfware::orchestration::workflows::WorkflowExecutor;

let executor = WorkflowExecutor::new();
executor.load_yaml(workflow_yaml)?;
let result = executor.execute("my-workflow", inputs, working_dir).await?;
```

---

## ✅ Working: Swarm Coordination

**Location**: `src/orchestration/swarm/`

**Status**: Functional, in-memory (no persistence).

**Features**:
- Agent pool management with roles and trust scores
- Decision making with voting and consensus
- Conflict resolution strategies (PriorityWins, ConfidenceWins, MajorityWins, etc.)
- Task queue with priority ordering
- Resource pressure integration
- Shared memory for agent communication

**Limitations**:
- All state is in-memory; no persistence across restarts
- Agents don't actually execute tasks (framework only)

---

## ✅ Working: Coordinator Mode

**Location**: `src/orchestration/swarm/coordinator.rs` (Swarm coordinator) and `src/orchestration/multiagent/interactive.rs` (`MultiAgentChat::interactive_swarm`)

**Status**: Functional beta — the swarm coordinator assigns each task to role-matched agents, and execution (via `MultiAgentChat`) is gated on that assignment. LLM calls are real; agents do not use tools.

**How it works**:
- Started with the `--coordinator` flag together with `multi-chat` (selects `MultiAgentChat::interactive_swarm()`)
- Builds a dev swarm via `create_dev_swarm()` (Architect, Coder, Tester, Reviewer)
- Each user task is queued as a `SwarmTask`; the coordinator assigns it to role-matched idle agents by trust score
- Execution runs only for the assignment, through `MultiAgentChat::run_task` — the same single-shot execution path as multi-agent chat (one non-streaming completion per agent, no tools)
- Results are fed back to the swarm via `complete_task`

**Interactive commands**: `/agents`, `/status`, `/parallel N`, `/clear`, `exit`

**Limitations**:
- Swarm state is in-memory; no persistence across restarts

---

## Summary Table

| Component | Status | Persistence | LLM Integration | Maturity |
|-----------|--------|-------------|-----------------|----------|
| Multi-Agent Chat | ✅ Working | N/A (stateless) | ✅ Real calls, no tool use | ⚠️ Beta — single-shot fan-out |
| Workflow Engine | ✅ Working | YAML files | Via handler | ✅ Production ready |
| Swarm Coordination | ✅ Working | ❌ In-memory only | ❌ Framework only | ⚠️ Partial (framework only) |
| Coordinator Mode | ✅ Working | ❌ In-memory only | ✅ Real calls (via MultiAgentChat) | ⚠️ Beta |

---

## Recommendations

### For Production Use

1. **Use Multi-Agent Chat** to fan the same prompt out to several role perspectives and compare the answers
2. **Use Workflow Engine** for structured, repeatable processes
3. **Use Coordinator Mode** (`--coordinator`) when you want the swarm coordinator to gate task execution on role-based agent assignment

### For Development

1. Add persistence to Swarm if you need long-running swarm coordination
2. If agents need to act on the codebase (read files, run commands), that requires wiring the tool registry into the multi-agent execution path — today they only chat

---

*Last updated: 2026-07-17*
