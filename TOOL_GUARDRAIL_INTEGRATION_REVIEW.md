# Tool and Guardrail Integration Review

**Date:** 2026-03-27  
**Reviewer:** Code Review Agent  
**Scope:** `src/swl/guardrails/` and `src/swl/runtime/`

---

## Executive Summary

The codebase has a **significant architectural gap**: while both tool registry integration and guardrail enforcement systems are implemented, they are **not integrated** in the active runtime. The `GuardedSwlRuntime` that would provide this integration is currently **disabled** due to compilation issues.

### Current Status
| Component | Status | Notes |
|-----------|--------|-------|
| Tool Registry | ✅ Active | Fully functional in `SwlRuntime` |
| Guardrail Types | ⚠️ Disabled | Module commented out in `src/swl/mod.rs` |
| Guardrail Engine | ⚠️ Disabled | Complete but unreachable |
| Guardrail Enforcer | ⚠️ Disabled | Complete but unreachable |
| Integration Runtime | ❌ Disabled | `GuardedSwlRuntime` commented out |

---

## 1. Tool Registry Integration

### 1.1 Current Implementation

**Location:** `src/swl/runtime/mod.rs` (lines 232-275, 792-919)

The base `SwlRuntime` integrates with `ToolRegistry`:

```rust
pub struct SwlRuntime {
    client: Arc<ApiClient>,
    tool_registry: Arc<ToolRegistry>,  // ✅ Tool registry integrated
    context: Arc<Mutex<ExecutionContext>>,
    // ...
}
```

**Key Features:**
- ✅ Tool registry passed via constructor (`with_tool_registry`)
- ✅ Per-agent tool filtering via `get_agent_tools()` (line 793-800)
- ✅ Tool definitions built for API calls (`build_tool_definitions`, line 862-878)
- ✅ Dual-format tool call support:
  - Native API tool calls (`execute_tool_call`, line 881-900)
  - XML-parsed tool calls (`execute_parsed_tool_call`, line 903-919)
- ✅ Tool availability validation before execution

### 1.2 Tool Execution Flow

```
Agent Execution
    ↓
Build System Prompt with Tool Definitions
    ↓
LLM Call with Tools
    ↓
Parse Tool Calls (Native or XML)
    ↓
Validate Tool is in Agent's Available List
    ↓
Execute via ToolRegistry
    ↓
Return Results to Agent
```

### 1.3 Gaps Identified

| Gap | Severity | Description |
|-----|----------|-------------|
| No Pre-Tool Guardrails | 🔴 High | No checkpoint before tool execution |
| No Post-Tool Guardrails | 🔴 High | No checkpoint after tool execution |
| No Tool Input Validation | 🟡 Medium | Only checks if tool is available, not arguments |
| No Tool Output Filtering | 🟡 Medium | Tool output goes directly to agent |

---

## 2. Guardrail Enforcement

### 2.1 Implementation Status

**Location:** `src/swl/guardrails/`

The guardrail system is **complete but disabled**:

| File | Lines | Status | Description |
|------|-------|--------|-------------|
| `types.rs` | 509 | ✅ Complete | All types defined: `GuardrailType`, `ViolationAction`, `GuardrailContext`, etc. |
| `engine.rs` | 891 | ✅ Complete | Condition evaluation engine with regex, comparisons, composites |
| `enforcer.rs` | 453 | ✅ Complete | `GuardrailEnforcer` with telemetry collection |
| `integration.rs` | 406 | ✅ Complete | Integration tests |
| `mod.rs` | 258 | ✅ Complete | Public API exports |

### 2.2 Guardrail Types Supported

```rust
pub enum GuardrailType {
    PreAgent,      // ✅ Implemented
    PostAgent,     // ✅ Implemented
    PreTool,       // ✅ Implemented (but unreachable)
    PostTool,      // ✅ Implemented (but unreachable)
    PreWorkflow,   // ✅ Implemented
    PostWorkflow,  // ✅ Implemented
}
```

### 2.3 Violation Actions

```rust
pub enum ViolationAction {
    Block,   // ✅ Halt execution
    Warn,    // ✅ Log warning, continue
    Log,     // ✅ Silent logging
    Alert,   // ✅ External notification
}
```

All four actions are fully implemented in `enforcer.rs` (lines 177-207):

```rust
match guardrail.on_violation {
    ViolationAction::Block => warn!(..."BLOCKING execution"),
    ViolationAction::Warn => warn!(..."WARNING issued"),
    ViolationAction::Log => info!(..."logged"),
    ViolationAction::Alert => warn!(..."ALERT triggered"),
}
```

### 2.4 Why It's Disabled

**Location:** `src/swl/mod.rs` (lines 24-26)

```rust
// Note: guardrails module has compilation issues, temporarily disabled
// pub mod guardrails;
```

The guarded runtime depends on this module:

```rust
// src/swl/runtime/mod.rs (lines 5-8)
// Note: guarded runtime depends on guardrails module which has compilation issues
// pub mod guarded;
// pub use guarded::{GuardedRuntimeBuilder, GuardedSwlRuntime};
```

---

## 3. The Guarded Runtime (Disabled)

### 3.1 What Would Be Provided

**Location:** `src/swl/runtime/guarded.rs`

The `GuardedSwlRuntime` would provide the integration:

```rust
pub struct GuardedSwlRuntime {
    base: SwlRuntime,                    // Tool support
    enforcer: Arc<Mutex<GuardrailEnforcer>>,  // Guardrail support
    current_workflow: Arc<Mutex<Option<String>>>,
}
```

### 3.2 Checkpoint Implementation

The guarded runtime implements **all 6 checkpoint types**:

| Checkpoint | Location in Code | Status |
|------------|------------------|--------|
| Pre-Workflow | Line 104-113 | ✅ Implemented |
| Post-Workflow | Line 141-144 | ✅ Implemented |
| Pre-Agent | Line 236-245 | ✅ Implemented |
| Post-Agent | Line 251-266 | ✅ Implemented |
| **Pre-Tool** | **Missing** | ❌ **Not Implemented** |
| **Post-Tool** | **Missing** | ❌ **Not Implemented** |

**Critical Gap:** While the guarded runtime exists, it has **placeholder agent execution** (line 442-466) that doesn't actually integrate with the base runtime's tool execution. The `execute_agent_with_guardrails` method is a stub.

---

## 4. Telemetry Collection

### 4.1 Tool Telemetry

**Location:** `src/swl/runtime/mod.rs` (lines 691-719)

The base runtime collects:
- ✅ Inference latency per agent call
- ✅ Token usage (input/output)
- ✅ API request counts
- ✅ Execution trace with events

```rust
let event = ExecutionEvent {
    timestamp: std::time::Instant::now(),
    agent: name.to_string(),
    action: "llm_call".to_string(),
    duration_ms,
    tokens_in,
    tokens_out,
};
```

### 4.2 Guardrail Telemetry

**Location:** `src/swl/guardrails/enforcer.rs` (lines 237-299)

The guardrail enforcer collects:
- ✅ Per-guardrail evaluation results
- ✅ Evaluation duration
- ✅ Action taken
- ✅ Agent/tool context
- ✅ Exportable as JSON

```rust
pub struct GuardrailTelemetryEvent {
    pub timestamp: String,
    pub guardrail_name: String,
    pub guardrail_type: String,
    pub result: String,
    pub action: String,
    pub duration_ms: u64,
    // ... optional fields
}
```

### 4.3 Telemetry Gap

There is **no unified telemetry** that correlates tool execution with guardrail checks. When both systems are active, tool calls and guardrail violations would be in separate telemetry streams.

---

## 5. Missing Integration Points

### 5.1 Tool-Specific Guardrails

Currently, guardrails can check:
- `agent_output` - Agent's text output
- `tool_input` - Tool arguments (intended for PreTool)
- `tool_output` - Tool results (intended for PostTool)

But these **cannot be enforced** because:
1. The guardrails module is disabled
2. The base runtime has no guardrail hooks
3. The guarded runtime has no actual tool execution integration

### 5.2 Required Integration

To properly integrate tools and guardrails, the following changes are needed:

```rust
// In SwlRuntime::execute_agent (around line 737-747)
// BEFORE executing tool:
if let Some(guardrails) = &self.guardrail_enforcer {
    let ctx = GuardrailContext::new()
        .with_current_tool(&tool_call.function.name)
        .with_tool_input(&tool_call.function.arguments);
    
    if let Some(blocking) = guardrails.should_block(GuardrailType::PreTool, &ctx).await? {
        return Err(SelfwareError::Safety(SafetyError::BlockedCommand { ... }));
    }
}

// AFTER tool execution:
let result = self.tool_registry.execute(...).await;

if let Some(guardrails) = &self.guardrail_enforcer {
    let ctx = GuardrailContext::new()
        .with_current_tool(&tool_call.function.name)
        .with_tool_output(&result.to_string());
    
    guardrails.check(GuardrailType::PostTool, &ctx).await?;
}
```

---

## 6. Testing Analysis

### 6.1 Existing Tests

| Test Suite | Count | Coverage |
|------------|-------|----------|
| `swl::runtime` | 17 | Tool registry, telemetry, execution context |
| `swl::guardrails::integration` | 15 | Guardrail enforcement, conditions, telemetry |

### 6.2 Test Gaps

**No tests exist for:**
- Tool execution with PreTool guardrails
- Tool execution with PostTool guardrails
- Guardrail blocking of dangerous shell commands
- Guardrail filtering of tool output
- Integration between tool telemetry and guardrail telemetry

### 6.3 Example Workflow Test

The `security_audit.swl` workflow (lines 402-433) defines guardrails:

```yaml
guardrails:
  - name: block_critical_vulnerabilities
    type: post_agent
    condition:
      language: rust
      content: |
        let findings = args.agent_outputs.owasp_findings.clone();
        !findings.contains("[CRITICAL]")
    on_violation: block
```

This **cannot be executed** with guardrail enforcement in the current codebase.

---

## 7. Recommendations

### 7.1 Immediate Actions (High Priority)

1. **Fix Guardrails Module Compilation**
   - Uncomment `pub mod guardrails;` in `src/swl/mod.rs`
   - Resolve any compilation errors
   - Enable `GuardedSwlRuntime`

2. **Integrate Tool Execution with Guardrails**
   - Modify `SwlRuntime::execute_agent` to accept optional `GuardrailEnforcer`
   - Add PreTool/PostTool checkpoints around tool execution
   - Or complete the `GuardedSwlRuntime::execute_agent_internal` implementation

3. **Add Critical Tool Safety Guardrails**
   ```rust
   // Pre-defined guardrail patterns for dangerous operations
   pub mod patterns {
       pub fn safe_shell_command() -> Condition { ... }
       pub fn allowed_paths(paths: &[&str]) -> Condition { ... }
       pub fn no_secrets_in_output() -> Condition { ... }
   }
   ```

### 7.2 Short-term Improvements (Medium Priority)

4. **Unified Telemetry**
   - Merge tool execution events with guardrail evaluation events
   - Add correlation IDs to trace tool call → guardrail check → outcome

5. **Workflow-Level Guardrail Testing**
   - Create integration tests using `security_audit.swl`
   - Test guardrail blocking with actual tool calls
   - Verify violation actions (block, warn, log, alert)

### 7.3 Long-term Enhancements (Lower Priority)

6. **Tool-Specific Guardrail Policies**
   - Define default guardrails per tool type
   - Allow workflow-specific overrides
   - Support guardrail inheritance

7. **Async Guardrail Evaluation**
   - Parallel evaluation of independent guardrails
   - Timeout handling for slow guardrail conditions
   - Circuit breaker for failing guardrail engines

---

## 8. Code Quality Assessment

### 8.1 Strengths

- ✅ Well-structured separation of concerns
- ✅ Comprehensive type system
- ✅ Good test coverage for individual components
- ✅ Proper error handling with `SafetyError` variants
- ✅ Telemetry collection is thorough

### 8.2 Issues

- 🔴 **Critical:** Guardrails completely disabled in production code
- 🟡 **Medium:** `GuardedSwlRuntime` has placeholder implementation
- 🟡 **Medium:** No unified telemetry correlation
- 🟢 **Low:** Some unused code warnings in guardrail engine

---

## 9. Security Implications

### 9.1 Current Risk Level: **HIGH**

Without guardrail enforcement:
- Agents can execute any tool without pre-validation
- Dangerous shell commands (`rm -rf`, `mkfs`, etc.) are not blocked
- Tool output containing secrets is not filtered
- No audit trail for tool execution decisions

### 9.2 Mitigation Path

1. Enable guardrails module (compilation fixes)
2. Implement tool execution hooks
3. Add default safety guardrails for shell/file tools
4. Test with security_audit.swl workflow

---

## Appendix: Key File Locations

| Component | File | Key Lines |
|-----------|------|-----------|
| Tool Registry | `src/tools/mod.rs` | 178-387 |
| Base Runtime | `src/swl/runtime/mod.rs` | 232-919 |
| Guarded Runtime | `src/swl/runtime/guarded.rs` | 1-587 (disabled) |
| Guardrail Types | `src/swl/guardrails/types.rs` | 1-509 |
| Guardrail Engine | `src/swl/guardrails/engine.rs` | 1-891 |
| Guardrail Enforcer | `src/swl/guardrails/enforcer.rs` | 1-453 |
| SWL Module | `src/swl/mod.rs` | 1-48 (disables guardrails) |
| Example Workflow | `workflows/security_audit.swl` | 402-433 (guardrail definitions) |

---

*End of Review*
