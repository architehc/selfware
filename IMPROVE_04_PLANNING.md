# IMPROVE-04: Planning & Orchestration — Stub Audit & Design Fixes

> Scope: `src/agent/`, `src/orchestration/`, `src/cognitive/`, `src/evolution/`  
> Date: 2026-04-28  
> Files examined: 33 files across 25+ modules  

---

## 1. Executive Summary

Selfware has **rich planning and orchestration infrastructure** (hierarchical planners, workflow engines, multi-agent chat, visual feedback loops, dream consolidation, and evolutionary optimization) but **critical execution paths are stubbed or un-wired**. The most severe blockers for automated benchmarking are:

1. **Coordinator workers do not execute anything** — they return placeholder strings.
2. **Dream consolidation is a no-op** — the background memory process is useless.
3. **Plan Mode has no LLM→Plan parser** — the state machine exists but is fed manually.
4. **Multi-agent chat is single-shot** — no tool execution loop, no reasoning.
5. **Evolution uses blocking HTTP inside async** and defaults to synthetic scores.

This document lists **every stub, TODO, and design gap with specific line numbers** and concrete fixes.

---

## 2. Critical Issues (Block Automated Benchmarks)

### 2.1 Coordinator Worker Execution is Purely Simulated
**File:** `src/orchestration/coordinator.rs`  
**Lines:** `864–901`

```rust
// Lines 864–901
async fn execute_task(
    id: &str,
    task: &str,
    role: &str,
    scratchpad: &Scratchpad,
) -> Result<String> {
    warn!("STUB: Worker {} (role: {}) SIMULATING task execution: {}", id, role, task);
    let output = format!(
        "STUB: Worker {} (role: {}) SIMULATED task: {}\n\
        ⚠️ NO ACTUAL EXECUTION - This is placeholder output.",
        id, role, task
    );
    // Store a STUB finding
    let finding_key = format!("finding:{}:{}", id, chrono::Utc::now().timestamp_millis());
    scratchpad.write(ScratchpadEntry::new(finding_key, "STUB: Sample finding...", id))?;
    Ok(output)
}
```

**Impact:** The `CoordinatorAgent` spawns background `tokio` tasks for Research → Synthesis → Implementation → Verification, but every worker returns the same formatted placeholder. Multi-agent orchestration cannot produce real results.

**Fix:** Wire `execute_task` to instantiate a real `Agent` (or at least an `ApiClient`) with the task prompt, run the full tool-execution turn loop, and return the agent’s final message. The `CoordinatorAgent` already holds a `Scratchpad` for sharing state; pass it into the agent as context.

---

### 2.2 Dream Consolidation Never Calls the LLM
**File:** `src/cognitive/dream_subprocess.rs`  
**Lines:** `385–414`

```rust
// Lines 385–414
async fn consolidate_phase(_memories: &[MemoryEntry], config: &AutoDreamConfig) -> Result<usize> {
    if _memories.is_empty() { return Ok(0); }
    let _prompt = crate::cognitive::dream::generate_consolidation_prompt(_memories);
    warn!("STUB: consolidate_phase NOT calling LLM at {} with model {}", config.endpoint, config.model);
    Ok(_memories.len()) // Returns count without actual consolidation
}
```

**Impact:** The AutoDream subprocess runs Orient → Gather → **Consolidate (stub)** → Prune. Without consolidation, duplicate or near-duplicate memories accumulate, and `MEMORY.md` never receives synthesized insights.

**Fix:** Use `config.endpoint` + `config.model` to POST the generated prompt (via `ApiClient` or `reqwest`). Parse the response into a list of deduplicated `MemoryEntry` objects and return the new count. Ensure the prompt requests structured output (e.g., JSON array of consolidated memories) for reliable parsing.

---

### 2.3 Plan Mode Manager Has No Plan Parser
**File:** `src/agent/plan_mode.rs`  
**Lines:** `256–376` (struct & state machine)  
**File:** `src/agent/plan_step.rs` (Agent::plan)  
**Lines:** `10–238`

**Gap:** `PlanModeManager` defines `Plan`, `PlanStep`, `StepStatus`, and a full state machine (`Inactive` → `Planning` → `Executing`). `Agent::plan()` makes an LLM call and returns `Ok(has_tool_calls)` — a boolean. **Nowhere in the codebase is there a function that parses an assistant’s text response into a `PlanModeManager::store_plan()` call.**

**Impact:** Plan Mode is only usable if a human (or external script) manually constructs `Plan` structs and injects them via `Agent::store_plan()`. The LLM cannot generate a plan autonomously.

**Fix:** Add a `parse_plan_from_llm(content: &str) -> Option<Plan>` function in `src/agent/plan_mode.rs`. Use a lightweight regex/JSON extractor (the prompt should ask the model to return a JSON object with `summary`, `files_to_read`, and `steps`). Wire it inside `Agent::plan()`:

```rust
// In Agent::plan(), after receiving assistant_msg:
if self.plan_mode {
    if let Some(plan) = plan_mode::parse_plan_from_llm(&content) {
        self.store_plan(plan);
    }
}
```

Also update the planning system prompt (in `Agent::build_planning_messages` or equivalent) to request structured plan output when `plan_mode` is enabled.

---

### 2.4 MultiAgentChat is Single-Shot with No Tool Loop
**File:** `src/orchestration/multiagent/chat.rs`  
**Lines:** `214–257` (`run_single_agent`), `255–257` (chat call)

```rust
// Lines 254–257
let result = tokio::time::timeout(timeout,
    client.chat(messages, None, ThinkingMode::Disabled)
).await;
```

**Impact:** Each agent gets exactly one LLM call with `ThinkingMode::Disabled`. The response is parsed for tool calls **from text only**, but they are **never executed**. The agent cannot read files, run tests, or verify its own output. Results are just aggregated strings.

**Fix:** Replace the single `client.chat()` call with a mini turn loop (or reuse `Agent::run_task()` if the agent struct can be instantiated per role):

1. Send initial messages.
2. While the model requests tool calls:
   - Execute each tool via the existing `ToolRegistry`.
   - Append tool results to the message history.
   - Re-call `client.chat()`.
3. Return the final assistant message.

If full `Agent` instantiation is too heavy, extract `Agent::execute_tools()` logic into a shared async helper that `MultiAgentChat` can call.

---

### 2.5 HierarchicalPlanner is Orphaned (660+ Lines of Dead Code)
**File:** `src/orchestration/planning.rs`  
**Lines:** `614–900` (`HierarchicalPlanner`), `922–1004` (`GoalDecomposer`)

**Gap:** `HierarchicalPlanner` has a full goal/plan/action hierarchy with PDVR (Plan-Do-Verify-Reflect) cycles, constraints, and static goal decomposers. **No non-test code references it.** Grepping the entire `src/` tree shows only definitions inside `planning.rs`.

**Impact:** Maintenance burden, compilation time, and cognitive overhead. Two different `Plan` structs exist (`plan_mode.rs:100` and `planning.rs:220`), risking confusion.

**Fix (choose one):**
- **Integrate:** Hook `HierarchicalPlanner` into `Agent::plan()` so the agent can decompose a high-level task into sub-goals, create plans, and execute actions from the queue. This is a large refactor.
- **Delete:** Remove `HierarchicalPlanner`, `GoalDecomposer`, `PdvrCycle`, and the duplicate `Plan`/`PlanStep` types. The simpler `PlanModeManager` is sufficient for explicit planning. If PDVR is needed later, re-implement it on top of `PlanModeManager`.

**Recommendation:** Delete. It is not wired, not tested in integration, and duplicates concepts already present in `PlanModeManager`.

---

## 3. Major Issues (Degrade Reliability / Performance)

### 3.1 Evolution Daemon Uses Blocking HTTP Inside Async
**File:** `src/evolution/daemon.rs`  
**Lines:** `868–899` (`call_llm`)

```rust
fn call_llm(llm: &LlmConfig, system_prompt: &str, user_prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    // POST to /chat/completions...
}
```

**Impact:** `call_llm` is a synchronous function using `reqwest::blocking::Client`. It is called from `generate_hypotheses()` which is called from the async `evolve()` loop. Blocking the async runtime for up to 300 seconds can stall the entire tokio executor.

**Fix:** Change `call_llm` to `async fn` and use `reqwest::Client` (non-blocking). Await the call inside `evolve()`. If the caller must remain sync for some reason, spawn it on a dedicated `tokio::task::spawn_blocking` thread.

---

### 3.2 Evolution Defaults to Synthetic Baseline Scores
**File:** `src/evolution/daemon.rs`  
**Lines:** `73–96` (env gate)

```rust
// Lines 73–96
if std::env::var("SELFWARE_EVOLVE_SAB").is_err() {
    // Use synthetic baseline scores
    return Ok(EvolutionResult {
        generations_run: 0,
        improvements: vec![],
        final_sab_score: 50.0,
        initial_sab_score: 50.0,
        total_duration: Duration::from_secs(0),
    });
}
```

**Impact:** Without the `SELFWARE_EVOLVE_SAB` environment variable, the evolution daemon exits immediately with fake scores. This makes it impossible to run as part of a standard CI/benchmark pipeline.

**Fix:** Remove the synthetic fallback gate. If the SAB runner script is missing, return a clear error instead of silently faking scores. Alternatively, auto-detect the SAB script presence and fail fast with instructions.

---

### 3.3 WorkflowExecutor Requires External Handler Injection
**File:** `src/orchestration/workflows.rs`  
**Lines:** `806–817` (struct definition), `1542–1555` (Tool step), `1582–1596` (LLM step)

```rust
// Lines 806–817
pub struct WorkflowExecutor {
    workflows: HashMap<String, Workflow>,
    tool_handler: Option<ToolHandler>,
    llm_handler: Option<LlmHandler>,
    dry_run: bool,
    safety_checker: crate::safety::SafetyChecker,
}
```

**Impact:** Workflows with `StepType::Tool` or `StepType::Llm` steps fail at runtime unless the caller manually injects handlers. In `cli/mod.rs`, the LLM handler is injected for SWL and YAML workflows, but **no `tool_handler` is ever injected** for YAML workflows (lines `1722–1816`).

**Fix:** Provide a `WorkflowExecutor::with_default_handlers(config: &Config)` constructor that wires:
- `llm_handler` → `ApiClient::chat()` (already done ad-hoc in CLI).
- `tool_handler` → a closure that looks up the tool in `ToolRegistry` and executes it.

This removes the injection burden from CLI callers and makes workflows usable out-of-the-box.

---

### 3.4 Visual Feedback Loop Has No Run Implementation
**File:** `src/orchestration/visual_loop.rs`  
**Lines:** `1–517` (entire file)

**Gap:** The file defines `VisualFeedbackLoop`, `VisualScore`, `CaptureMethod`, `VisualLoopResult`, and `build_critic_prompt()`, but **there is no `run()` method or any async loop implementation**. Grep for `fn run` or `impl VisualFeedbackLoop` returns nothing beyond the struct definition.

**Impact:** The autonomous visual iteration pipeline (act → capture → evaluate → iterate) is entirely spec-level. It cannot be invoked.

**Fix:** Implement `VisualFeedbackLoop::run(&self, task: &str, agent: &mut Agent) -> Result<VisualLoopResult>`. The loop should:
1. Call `agent.execute_task(task)` to generate/modify code.
2. Capture the visual output (screenshot via `browser_screenshot` or OS capture).
3. Send the image + critic prompt to a vision-capable model.
4. Parse the JSON response into `VisualScore`.
5. If `overall < quality_threshold`, inject the critic’s suggestions into the agent’s context and repeat (up to `max_iterations`).

---

### 3.5 Scratchpad is Coordinator-Only and Unintegrated
**File:** `src/orchestration/scratchpad.rs`  
**Line:** `8` (module header comment)

```rust
//! Used only by the coordinator module; pending full integration
```

**Impact:** The `Scratchpad` is a clean key/value + typed read/write abstraction for shared agent memory. It is only used inside the coordinator (and its tests). No other module persists findings across sessions.

**Fix:** Inject a `SharedScratchpad` (already defined as `Arc<RwLock<Scratchpad>>`) into `Agent`. After each tool-execution turn, write key findings (files read, errors encountered, tool calls made) to the scratchpad under a namespaced key (`agent:<session_id>:turn:<n>`). On session start, load prior scratchpad entries as context. This gives the agent episodic memory without relying on the heavy AutoDream subprocess.

---

## 4. Moderate Issues

### 4.1 Duplicate `Plan` Structs
**Files:**
- `src/agent/plan_mode.rs:100`
- `src/orchestration/planning.rs:220`

Both define `Plan` with steps, but different step types (`PlanStep` vs `PlanStep` with `GoalStatus`). This is confusing and leads to namespace collisions if both modules are imported.

**Fix:** If `HierarchicalPlanner` is deleted (recommended in §2.5), the duplicate disappears automatically. If kept, rename one to `HierarchicalPlan`.

### 4.2 Coordinator Tests Only Test Structure, Not Behavior
**File:** `src/orchestration/coordinator.rs`  
**Lines:** `1017–1523` (test module)

All tests verify enum serialization, scratchpad read/write, and worker lifecycle state transitions. **No test verifies that a worker produces real output** because the implementation is a stub.

**Fix:** Once `execute_task` is wired to a real agent, add an integration test that spawns a worker with a simple task (e.g., "Count lines in src/main.rs") and asserts the output contains actual file content, not the string "STUB".

### 4.3 Dream Subprocess Lacks Integration Test for Consolidation
**File:** `src/cognitive/dream_subprocess.rs`

The test module (if any) is minimal. There is no test asserting that `consolidate_phase` deduplicates memories.

**Fix:** After implementing the LLM call in `consolidate_phase`, add a test with synthetic memories containing duplicates and assert the returned count is lower than the input count.

---

## 5. Minor Issues

### 5.1 `PlanModeManager::is_tool_allowed` Ignores Tool Name
**File:** `src/agent/plan_mode.rs`  
**Lines:** `363–370`

```rust
pub fn is_tool_allowed(&self, _tool_name: &str, is_readonly: bool) -> bool {
    match self.state {
        PlanModeState::Inactive => true,
        PlanModeState::Planning { .. } => is_readonly,
        PlanModeState::Executing => true,
    }
}
```

The `_tool_name` parameter is unused. The `READONLY_TOOLS` list (lines `381–389`) exists but is never consulted. A tool marked as read-only by metadata but not in `READONLY_TOOLS` would still be allowed in planning phase if `is_readonly` is true, which is fine, but the explicit list is redundant.

**Fix:** Either remove `READONLY_TOOLS` or use it as the source of truth inside `is_tool_allowed`, removing the `is_readonly` parameter.

### 5.2 Evolution `build_metrics` Hard-Codes Fake Values
**File:** `src/evolution/daemon.rs`  
**Lines:** `1044–1058`

```rust
fn build_metrics(sab: &SabResult, config: &EvolutionConfig) -> FitnessMetrics {
    FitnessMetrics {
        test_coverage_pct: 82.0, // Would need real measurement
        binary_size_mb: 15.0,    // Would need real measurement
        tests_passed: 5200,
        tests_total: 5200,
        // ...
    }
}
```

These hard-coded values mislead the fitness function. If real measurement is not feasible, set them to `0.0` / `0` and document that they are unimplemented.

---

## 6. Recommended Fix Priority

| Priority | Issue | File | Lines | Effort |
|----------|-------|------|-------|--------|
| **P0** | Worker execution stub | `coordinator.rs` | `864–901` | Medium |
| **P0** | Dream consolidation stub | `dream_subprocess.rs` | `385–414` | Small |
| **P0** | Plan parser for Plan Mode | `plan_mode.rs` + `plan_step.rs` | New + `10–238` | Medium |
| **P1** | Multi-agent tool loop | `multiagent/chat.rs` | `214–257` | Medium |
| **P1** | Evolution blocking I/O | `evolution/daemon.rs` | `868–899` | Small |
| **P1** | Evolution synthetic gate | `evolution/daemon.rs` | `73–96` | Tiny |
| **P1** | Delete or integrate HierarchicalPlanner | `orchestration/planning.rs` | `614–900` | Medium |
| **P2** | Workflow default handlers | `workflows.rs` + `cli/mod.rs` | `806–817` | Small |
| **P2** | Visual loop implementation | `visual_loop.rs` | New | Large |
| **P2** | Scratchpad integration | `scratchpad.rs` + `agent/mod.rs` | New | Medium |

---

## 7. Concrete Code Snippets for Fixes

### 7.1 Minimal Plan Parser (for Plan Mode)

Add to `src/agent/plan_mode.rs` after line 376:

```rust
/// Parse a plan from an LLM response that contains a JSON code block.
/// Expected format:
/// ```json
/// {
///   "summary": "...",
///   "files_to_read": ["src/main.rs"],
///   "steps": [
///     {"description": "Read main.rs", "tool_hint": "file_read", "file_path": "src/main.rs"}
///   ]
/// }
/// ```
pub fn parse_plan_from_llm(content: &str) -> Option<Plan> {
    let json_block = content
        .split("```json")
        .nth(1)?
        .split("```")
        .next()?;
    let raw: serde_json::Value = serde_json::from_str(json_block).ok()?;

    let mut plan = Plan::with_summary(raw.get("summary")?.as_str()?);
    if let Some(files) = raw.get("files_to_read").and_then(|f| f.as_array()) {
        for f in files {
            if let Some(s) = f.as_str() {
                plan.add_file_to_read(s);
            }
        }
    }
    if let Some(steps) = raw.get("steps").and_then(|s| s.as_array()) {
        for step_val in steps {
            let desc = step_val.get("description")?.as_str()?;
            let mut step = PlanStep::new(plan.steps.len() + 1, desc);
            if let Some(hint) = step_val.get("tool_hint").and_then(|h| h.as_str()) {
                step.tool_hint = Some(hint.to_string());
            }
            if let Some(fp) = step_val.get("file_path").and_then(|p| p.as_str()) {
                step.file_path = Some(fp.to_string());
            }
            plan.steps.push(step);
        }
    }
    Some(plan)
}
```

Then in `src/agent/plan_step.rs`, after line 102 (`let content = &assistant_msg.content;`):

```rust
if self.plan_mode {
    if let Some(plan) = plan_mode::parse_plan_from_llm(content.text()) {
        self.store_plan(plan);
        self.plan_mode_manager.approve_plan(); // or require explicit user approval
    }
}
```

### 7.2 Async `call_llm` for Evolution

Replace lines `868–899` in `daemon.rs`:

```rust
async fn call_llm(
    llm: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    // ... async POST ...
    let response = client
        .post(&llm.endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    // ... parse ...
}
```

And await it in `generate_hypotheses()`:

```rust
let response = call_llm(&config.llm, &system_prompt, &user_prompt).await?;
```

### 7.3 Workflow Default Tool Handler

Add a method to `WorkflowExecutor` in `workflows.rs`:

```rust
pub fn with_default_tool_handler(self, registry: Arc<ToolRegistry>) -> Self {
    self.with_tool_handler(Box::new(move |name, args| {
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|e| anyhow!("No tokio runtime: {}", e))?;
        rt.block_on(async {
            registry.execute(name, args).await
                .map(|r| r.to_string())
                .map_err(|e| anyhow!("Tool error: {}", e))
        })
    }))
}
```

Then in `cli/mod.rs`, when creating the executor for YAML/SWL runs, call `.with_default_tool_handler(tool_registry)`.

---

## 8. Summary for Benchmark Automation

To make Selfware capable of running **SWE-bench or similar automated benchmarks** end-to-end, the following must be true:

1. **Planning produces structured, parseable output** → Fix §2.3 (plan parser).
2. **Agents can execute tools autonomously** → Already works for single agent; fix §2.4 (multi-agent loop) only if using multi-agent mode.
3. **Coordinator workers execute real code** → Fix §2.1 (worker stub).
4. **Background memory consolidation works** → Fix §2.2 (dream stub).
5. **No blocking I/O in async paths** → Fix §3.1 (evolution blocking HTTP).
6. **No silent fake scores** → Fix §3.2 (evolution synthetic gate).

The **fastest path to a working benchmark** is:
- Use the **single `Agent` loop** (already functional) rather than the coordinator or multi-agent chat.
- Enable **Plan Mode** with the JSON plan parser (§7.1).
- Ensure the agent can read files, edit files, and run shell commands in a loop.
- Use the **existing tool execution and safety systems** — they are already wired and tested.

The coordinator, multi-agent chat, evolution daemon, and visual loop are **advanced features that should be considered experimental until their stubs are filled**.
