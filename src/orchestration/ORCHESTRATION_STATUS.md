# Orchestration Module Status

This document describes the current status of the orchestration module - what works and what's planned.

## Overview

The orchestration module provides multi-agent coordination capabilities for the selfware project. It includes several subsystems:

- **Multi-Agent Chat** (`multiagent/`): ✅ Production Ready
- **Workflow Engine** (`workflows.rs`): ✅ Production Ready  
- **Swarm Coordination** (`swarm/`): ✅ Functional (in-memory)
- **Coordinator Mode** (`swarm/coordinator.rs` + `multiagent/interactive.rs`): ✅ Functional — execution routes through `MultiAgentChat`
- **Scratchpad** (`scratchpad.rs`): ✅ Functional

---

## ✅ Working: Multi-Agent Chat

**Location**: `src/orchestration/multiagent/`

**Status**: Production ready and actively used.

**Features**:
- Spawn multiple agents with different roles (Coder, Reviewer, Tester, Architect)
- Concurrent execution with configurable concurrency limits
- Event streaming for real-time UI updates
- Failure policies (FailFast, ContinueOnError)
- Health monitoring with heartbeats
- Result aggregation

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

**Status**: Functional — task execution routes through `MultiAgentChat` (real LLM calls and tool use, no simulation).

**How it works**:
- Started with the `--coordinator` flag on `multichat` (selects `MultiAgentChat::interactive_swarm()`)
- Builds a dev swarm via `create_dev_swarm()` (Architect, Coder, Tester, Reviewer) with `ConfidenceWins` conflict strategy and a 0.5 consensus threshold
- Each user task is queued as a `SwarmTask` and assigned to role-matched idle agents by trust score
- Execution runs through `MultiAgentChat::run_task` — the same production execution path as multi-agent chat
- Results are fed back to the swarm via `complete_task`, and agents vote on the best response in a consensus decision

**Interactive commands**: `/agents`, `/status`, `/parallel N`, `/clear`, `exit`

**Limitations**:
- Swarm state is in-memory; no persistence across restarts

---

## ✅ Working: Scratchpad

**Location**: `src/orchestration/scratchpad.rs`

**Status**: Functional with disk persistence.

**Features**:
- Key-value storage for coordinator-worker communication
- JSON persistence to `~/.local/share/.selfware/scratchpad/{task_id}/`
- Worker registration and status tracking
- Typed value storage via serde
- Thread-safe operations

**Usage**:
```rust
use selfware::orchestration::Scratchpad;

let scratchpad = Scratchpad::for_task("my-task")?;
scratchpad.write(ScratchpadEntry::new("key", "value", "author"))?;
let entry = scratchpad.read("key");
```

---

## Summary Table

| Component | Status | Persistence | LLM Integration | Production Ready |
|-----------|--------|-------------|-----------------|------------------|
| Multi-Agent Chat | ✅ Working | N/A (stateless) | ✅ Full | ✅ Yes |
| Workflow Engine | ✅ Working | YAML files | Via handler | ✅ Yes |
| Swarm Coordination | ✅ Working | ❌ In-memory only | ❌ Framework only | ⚠️ Partial |
| Coordinator Mode | ✅ Working | ❌ In-memory only | ✅ Full (via MultiAgentChat) | ✅ Yes |
| Scratchpad | ✅ Working | ✅ JSON files | N/A | ✅ Yes |

---

## Recommendations

### For Production Use

1. **Use Multi-Agent Chat** for parallel agent execution
2. **Use Workflow Engine** for structured, repeatable processes
3. **Use Coordinator Mode** (`--coordinator`) when you want swarm task assignment and consensus voting on top of multi-agent execution

### For Development

1. Add persistence to Swarm if you need long-running swarm coordination

---

*Last updated: 2026-07-17*
