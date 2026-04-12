# Code Quality & Rust Health Audit Report

**Agent:** Agent 3 - Code Quality & Rust Health  
**Repository:** architehc/selfware  
**Branch:** agent-20260405-145152  
**Audit Date:** 2026-04-09  
**Total Lines of Code:** ~200,861

---

## Executive Summary

The Selfware codebase shows a **mixed quality profile** with solid safety infrastructure but concerning patterns in error handling, significant technical debt markers, and notable placeholder implementations that appear functional but are incomplete.

### Key Findings at a Glance

| Metric | Count | Risk Level |
|--------|-------|------------|
| `.unwrap()` calls (production) | ~558 | 🔴 HIGH |
| `.unwrap()` calls (total) | 3,440 | 🟡 MEDIUM |
| `panic!()` calls | 119 | 🟡 MEDIUM |
| `TODO` comments | ~15 | 🟡 MEDIUM |
| `unsafe` blocks | 3 | 🟢 LOW |
| Placeholder/stub modules | 5+ | 🔴 HIGH |
| Files analyzed | 141 | - |

---

## 1. TODO/FIXME/HACK Inventory

### Critical TODOs (Require Immediate Attention)

| Location | Line | Comment | Severity |
|----------|------|---------|----------|
| `src/swl/guardrails/engine.rs` | 421 | `// TODO: Implement JSON Logic evaluation` | 🔴 **CRITICAL** |
| `src/cognitive/dream_subprocess.rs` | 406 | `// TODO: Implement actual LLM call for consolidation` | 🔴 **CRITICAL** |
| `src/evolution/telemetry.rs` | 227 | `// TODO: Integrate DHAT or custom allocator tracking` | 🟡 MEDIUM |
| `src/cognitive/memory_hierarchy/mod.rs` | 110 | `// TODO: Add content summarization per file` | 🟡 MEDIUM |
| `src/cognitive/memory_hierarchy/mod.rs` | 111 | `// TODO: Extract symbol-level context` | 🟡 MEDIUM |
| `src/agent/execution.rs` | 477 | `// TODO: implement the functions described in the task` | 🟢 LOW (template) |

### Assessment

- **JSON Logic evaluation** is explicitly unimplemented and returns an error. This breaks guardrail functionality for JSON-based conditions.
- **Dream subprocess consolidation** is a placeholder that does not actually call the LLM, making memory consolidation non-functional.
- **Telemetry allocation profiling** is acknowledged as P2 feature but marked TODO.

### No FIXME or HACK comments found

This is positive - the codebase doesn't contain explicit "FIXME" or "HACK" markers, though the TODOs indicate incomplete features.

---

## 2. Error Handling Audit

### Unwrap Usage Analysis

**Production code (excluding tests):** ~558 unwrap calls  
**Total including tests:** 3,440 unwrap calls

#### Files with Highest Unwrap Counts (Production)

| File | Estimated Unwraps | Risk |
|------|-------------------|------|
| `src/orchestration/coordinator.rs` | 40+ | 🔴 HIGH |
| `src/orchestration/scratchpad.rs` | 30+ | 🔴 HIGH |
| `src/agent/*.rs` (combined) | 150+ | 🔴 HIGH |
| `src/tools/*.rs` (combined) | 200+ | 🔴 HIGH |

#### Pattern Analysis

**Problematic patterns found:**

1. **Test code using unwrap extensively** - Acceptable for tests but 2,882 unwraps in tests suggests some production code may be untested
2. **Production unwrap patterns:**
   - `serde_json::to_string(&x).unwrap()` - Common for serialization (usually safe but poor practice)
   - `path.file_name().unwrap()` - Potential panic on invalid paths
   - ` Arc::new(ApiClient::new(&config).expect("...")` - Startup failures

#### Panic Usage

119 `panic!()` calls found, mostly in:
- Test assertions (acceptable)
- **Workflow DSL parser** (`src/orchestration/workflow_dsl/`) - Uses panic for "impossible" states in AST parsing (lines 617-730)
- **AST module** - Multiple panic calls for unwrapping node types (lines 151-461)

**Concern:** The workflow DSL parser contains ~30 panic calls for supposedly "impossible" parser states. These could be triggered by malformed input.

### Unsafe Blocks

Only **3 actual unsafe blocks** found in production code:

| Location | Line | Usage | Assessment |
|----------|------|-------|------------|
| `src/tools/hot_reload.rs` | 45 | Library loading | 🟡 Justified with comment |
| `src/tools/hot_reload.rs` | 108 | Library loading | 🟡 Justified with comment |
| `src/safety/path_validator.rs` | 50 | Path operations | 🟡 Wrapped in unsafe block |
| `src/ui/tui/mod.rs` | 131 | Signal handling | 🟡 Justified for sigaction |

**Assessment:** Unsafe usage is minimal and appears justified with comments explaining necessity.

---

## 3. Safety/Supervision Assessment

### Safety Module (`src/safety/`)

**Status: SUBSTANTIALLY IMPLEMENTED** ✅

The safety infrastructure is surprisingly complete:

#### What's Actually Working:

1. **Safety Checker** (`src/safety/checker/validation.rs`)
   - Comprehensive tool call validation for 50+ tools
   - Shell command normalization and dangerous pattern detection
   - Path traversal prevention
   - SSRF protection with cloud metadata blocking
   - Secret scanning integration
   - Container volume mount validation

2. **Security Scanner** (`src/safety/scanner.rs`)
   - Pattern-based secret detection
   - Multiple security categories (secrets, injections, unsafe code)
   - Severity classification

3. **Sandbox** (`src/safety/sandbox.rs`)
   - Filesystem policies (allow/deny lists, extensions)
   - Network policies (firewall rules, localhost handling)
   - Resource limits (CPU, memory, timeouts)
   - Audit logging with risk levels
   - Autonomy levels (SuggestOnly → FullAutonomous)

4. **Path Validator** (`src/safety/path_validator.rs`)
   - Symlink attack prevention
   - Protected path enforcement
   - Working directory validation

#### Critical Gap Found:

**JSON Logic evaluation in Guardrails** (`src/swl/guardrails/engine.rs:421`):
```rust
fn evaluate_json_logic(&self, _logic: &str, _context: &GuardrailContext) -> EvaluationResult {
    // TODO: Implement JSON Logic evaluation
    EvaluationResult::Error {
        message: "JSON Logic evaluation not yet implemented".to_string(),
    }
}
```

**Impact:** Guardrails using JSON Logic conditions will fail evaluation.

### Supervision Module (`src/supervision/`)

**Status: MOSTLY STUBBED** ⚠️

#### Circuit Breaker (`src/supervision/circuit_breaker.rs`)

**Status: FULLY IMPLEMENTED** ✅

- Proper state machine (Closed/Open/HalfOpen)
- Configurable thresholds and timeouts
- Exponential backoff
- Comprehensive test coverage (22 tests)

#### Health Monitor (`src/supervision/health.rs`)

**Status: FUNCTIONAL** ✅

- Health check trait with async support
- Built-in checks: Agent heartbeat, GPU, Memory, Disk
- HTTP health endpoint for Docker/K8s
- Overall status aggregation

#### Supervisor (`src/supervision/mod.rs`)

**Status: PARTIALLY STUBBED** ⚠️

The `Supervisor` struct has a critical implementation gap:

```rust
async fn restart_child(&self, child_id: &str) {
    info!(child_id = %child_id, "Restarting child");
    // In a real implementation, this would restart the actual component
}

async fn escalate(&self, child_id: &str, error: &str) {
    error!(child_id = %child_id, error = %error, "Escalating failure");
    // In a real implementation, this would notify a parent supervisor
}
```

**Impact:** The supervision tree logs events but doesn't actually restart failed components or escalate to parent supervisors.

---

## 4. Placeholder/Vaporware Inventory

### Critical Placeholders (Appear functional but don't work)

| Module | File | Description | Severity |
|--------|------|-------------|----------|
| Batch Mode | `src/batch/mod.rs` | Line 1: "PLACEHOLDER — executor is stubbed". `execute_single_task` just returns the task string | 🔴 **CRITICAL** |
| Visual Validation | `src/validation/mod.rs` | Line 1: "PLACEHOLDER — analysis returns fabricated scores". Analysis always returns hardcoded score 7.5 | 🔴 **CRITICAL** |
| SWL Guardrails | `src/swl/guardrails/engine.rs` | JSON Logic evaluation returns error | 🔴 **CRITICAL** |
| Dream Consolidation | `src/cognitive/dream_subprocess.rs` | Memory consolidation is a no-op placeholder | 🟡 HIGH |
| SWE-bench | `src/cli/mod.rs` | Line 1825: "SWE-bench evaluation is not implemented yet" | 🟡 HIGH |
| Visual Validation CLI | `src/cli/mod.rs` | Line 1222: "Visual validation is not fully wired yet" | 🟡 HIGH |

### Assessment

These modules export functional-looking APIs but contain placeholder implementations:

1. **Batch Mode** - Creates the impression of parallel task execution but tasks don't actually run
2. **Visual Validation** - Screenshot capture works via Playwright, but analysis always returns fabricated scores
3. **SWL Guardrails** - Most conditions work, but JSON Logic (common standard) is explicitly not implemented

---

## 5. Code Quality Issues by Severity

### 🔴 CRITICAL (Memory Safety, Security, Data Loss)

| Issue | Location | Details |
|-------|----------|---------|
| JSON Logic guardrails fail | `src/swl/guardrails/engine.rs:421` | Returns error for valid JSON Logic conditions |
| Visual validation fabricates scores | `src/validation/mod.rs:183` | Hardcoded 7.5 score, not using vision model |
| Batch executor doesn't execute | `src/batch/mod.rs:165` | Placeholder returns task string instead of executing |
| Path validation unsafe | `src/safety/path_validator.rs:50` | Uses unsafe block without detailed justification |

### 🔴 HIGH (Likely Bugs, Unwrap in Production)

| Issue | Location | Details |
|-------|----------|---------|
| Workflow DSL panics on bad input | `src/orchestration/workflow_dsl/*.rs` | ~30 panics for "impossible" parser states |
| Supervisor doesn't restart | `src/supervision/mod.rs:208` | Logs restart but doesn't actually restart |
| Excessive unwrap in orchestration | `src/orchestration/` | 200+ unwraps across coordinator, scratchpad |
| Dream memory consolidation no-op | `src/cognitive/dream_subprocess.rs:402` | Placeholder doesn't consolidate |

### 🟡 MEDIUM (Tech Debt, Maintainability)

| Issue | Location | Details |
|-------|----------|---------|
| 558 unwraps in production | Throughout | Should use proper error propagation |
| TODO markers for features | Various | 15+ TODOs indicating incomplete work |
| Hardcoded placeholder values | Various | 10GB disk placeholder, 100MB container size placeholder |
| Supervisor escalation stub | `src/supervision/mod.rs:246` | Doesn't actually escalate failures |

### 🟢 LOW (Style, Formatting)

| Issue | Location | Details |
|-------|----------|---------|
| rustfmt.toml minimal | Root | Only 4 lines, could be more comprehensive |
| Clippy config permissive | `clippy.toml` | High thresholds may mask complexity issues |
| Commented-out code | Various | Some test files contain commented code |

---

## 6. Config File Analysis

### `clippy.toml`

```toml
too-many-arguments-threshold = 10
type-complexity-threshold = 350
cognitive-complexity-threshold = 30
```

**Assessment:** Permissive thresholds that may allow overly complex functions. The cognitive complexity threshold of 30 is quite high (default is usually 25).

### `rustfmt.toml`

```toml
edition = "2021"
max_width = 100
use_field_init_shorthand = true
use_try_shorthand = true
```

**Assessment:** Minimal but reasonable configuration. `use_try_shorthand = true` encourages `?` operator usage.

---

## 7. Recommendations

### Immediate Actions (This Sprint)

1. **Fix JSON Logic evaluation** in `src/swl/guardrails/engine.rs`
   - Implement proper JSON Logic evaluation or remove the feature flag
   - Currently breaks guardrail contracts

2. **Add warning labels** to placeholder modules:
   - `src/batch/mod.rs` - Add prominent documentation that it's stubbed
   - `src/validation/mod.rs` - Document that analysis scores are fabricated

3. **Audit all unwrap() calls** in production paths:
   - Focus on `src/orchestration/coordinator.rs`
   - Replace with proper error handling using `?` operator

### Short-term (Next 2 Sprints)

4. **Complete Supervisor implementation**:
   - Actually implement `restart_child()` to restart components
   - Implement `escalate()` to notify parent supervisors

5. **Reduce unwrap() count** by 50% in production code:
   - Target: < 250 unwraps in production
   - Use `?` operator with custom error types

6. **Fix Dream subprocess consolidation**:
   - Wire up actual LLM call for memory consolidation
   - Or remove the feature if not planned

### Long-term (This Quarter)

7. **Implement proper error types** throughout:
   - Reduce reliance on `anyhow` in library code
   - Use `thiserror` for structured error types

8. **Add fuzz testing** for Workflow DSL parser:
   - The panic-based error handling needs fuzz testing
   - Ensure malformed input doesn't crash the system

9. **Complete or remove placeholder features**:
   - Visual validation scoring
   - Batch mode execution
   - SWE-bench evaluation

---

## 8. Summary Grades

| Category | Grade | Notes |
|----------|-------|-------|
| Safety Infrastructure | B+ | Comprehensive but JSON Logic gap |
| Supervision | C+ | Circuit breaker good, supervisor stubbed |
| Error Handling | C | Too many unwraps, panic in parser |
| Documentation | B | Good module docs, some placeholder gaps |
| Testing | B+ | Extensive test coverage |
| Placeholder Transparency | D | Several modules appear functional but aren't |

**Overall Code Health: C+**

The codebase has solid foundations in safety and testing but suffers from:
1. Excessive use of unwrap in production paths
2. Several feature modules that are stubs but don't advertise it
3. Critical gaps in supervision and guardrail functionality

---

*Report generated by Agent 3 - Code Quality & Rust Health Audit*  
*For questions or clarifications, contact the development team*
