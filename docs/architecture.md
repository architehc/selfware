# Selfware Architecture

This document maps the main runtime flow and module boundaries for contributors.

## High-Level Module Map

```text
┌───────────────────────────────────────────────────────────────────────┐
│                               CLI / UI                               │
│  src/main.rs, src/input, src/ui, src/orchestration                  │
└───────────────────────────────┬───────────────────────────────────────┘
                                │
                                ▼
┌───────────────────────────────────────────────────────────────────────┐
│                              Agent Core                              │
│  src/agent (loop control, planning, context compression, execution)  │
└───────────────┬───────────────────────────────┬───────────────────────┘
                │                               │
                ▼                               ▼
┌──────────────────────────────┐      ┌────────────────────────────────┐
│         Tool System          │      │            API Layer           │
│  src/tools + ToolRegistry    │      │  src/api (chat + streaming)    │
└───────────────┬──────────────┘      └────────────────────────────────┘
                │
                ▼
┌───────────────────────────────────────────────────────────────────────┐
│                      Safety + Verification Guardrails                 │
│  src/safety (checker/path validator/scanner)                          │
│  src/verification (cargo/test/lint verification gate)                 │
└───────────────────────────────────────────────────────────────────────┘
```

## PDVR Cycle (Plan, Do, Verify, Reflect)

```text
User Task
  │
  ▼
Plan
  - Build prompt + contextual state
  - Decide next actions / tools
  │
  ▼
Do
  - Parse tool calls (native FC or parser fallback)
  - Safety-check each call
  - Execute tool calls
  │
  ▼
Verify
  - Run verification gate for code-changing actions
  - Record pass/fail signals
  │
  ▼
Reflect
  - Update episodic/working memory
  - Decide whether to continue or finalize
```

## Tool Registration Flow

```text
ToolRegistry::new()
  ├─ registers built-in tool implementations
  ├─ exposes schemas/descriptions to model prompts
  └─ resolves name -> tool instance at execution time
```

Execution path:

1. Model emits tool call (native function call or parsed format).
2. `SafetyChecker` validates call arguments and shell/path risk.
3. `ToolRegistry` resolves the tool by name.
4. Tool executes and result is fed back to the model as tool result context.

## Safety Validation Pipeline

```text
Incoming ToolCall
  │
  ├─ file_* tools  ─► PathValidator
  │                  - path traversal checks
  │                  - denied/allowed glob checks
  │                  - symlink chain safety
  │
  ├─ shell_exec    ─► command pattern scanner
  │                  - destructive command regexes
  │                  - chain/shell-obfuscation detection
  │
  └─ git_push      ─► force-push guard
```

## Key Contributor Notes

- `src/main.rs` should stay a thin CLI entrypoint that delegates to library modules.
- `src/agent/mod.rs` is currently the largest critical path file; refactors should preserve loop behavior and safety semantics.
- Prefer adding behavior through focused submodules (`agent/*`, `safety/*`) rather than growing central files.
