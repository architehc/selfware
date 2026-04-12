# Selfware Codebase Audit — Round 2 (Validation Pass)

**Date:** April 10, 2026
**Branch:** agent-20260405-145152
**Method:** 5 parallel Claude agents validating Round 1 claims against actual code, compiler, and live endpoint

Round 1 was a broad discovery pass across 6 domains. Round 2 focused on **validation**: compile things, curl the endpoint, read the actual code behind "vaporware" labels, and triage the unwrap panic risk properly. Several Round 1 findings were wrong or overstated.

---

## Corrections to Round 1

| # | Round 1 Claim | Round 2 Verdict | Evidence |
|---|---------------|------------------|----------|
| 1 | Default endpoint `crazyshit.ngrok.io` is dead | **REFUTED** | Endpoint is live and serving Qwen3.5-122B-A10B-NVFP4-yarn-1010k (1M context) |
| 2 | 3,450 unwraps are a CRITICAL crash risk | **OVERSTATED** | Raw count confirmed, but agent execution path has **zero** production unwraps; ~55 real high-risk sites total |
| 3 | SWL workflow system is not wired into CLI | **REFUTED** | `cli/mod.rs:1354-1402` actually calls `executor.execute()` on lowered SWL document |
| 4 | Evolution engine never called | **REFUTED** | `cli/mod.rs:1084-1150` starts evolution daemon with full config (feature-gated) |
| 5 | `consolidation` feature gate breaks `--no-default-features` | **REFUTED (partial)** | `cargo check --no-default-features` passes clean; may still fail with `--tests` |
| 6 | bench-harness has 1 compile error (TaskSetup re-export) | **UNDERSTATED** | Has **14 errors** — TaskSetup is just one; also missing Path import, missing Hash derive (cascades 10x), turbofish issue |
| 7 | Native function calling should be `true` for SGLang configs | **WRONG DIRECTION** | SGLang endpoint emits XML `<tool_call>` in `content`, NOT native JSON `tool_calls[]`. Correct value is **`false`** with a Qwen XML parser |
| 8 | Orchestration `coordinator.rs` rated "Active MIXED" | **DOWNGRADED** | Compiler confirms `coordinator` + `scratchpad` have **no constructors called anywhere** — they are dead code (21 of 27 default warnings) |
| 9 | `selfware.toml` endpoint must be changed | **REVERSED** | No change needed — endpoint works as-is; keep `https://crazyshit.ngrok.io/v1` |

---

## Newly Confirmed Findings

### 1. Build Health (Agent A)

| Target | Result |
|--------|--------|
| `cargo check` (default) | PASS — 0 errors, 27 warnings |
| `cargo check --no-default-features` | PASS — 0 errors |
| `cargo check --features bench-harness` | **FAIL — 14 errors** |
| `cargo check --all-features` | FAIL — same 14 errors |
| `cargo build --release` | PASS — binary builds |
| `cargo clippy` | 45 issues (mostly low-severity) |

**bench-harness feature is broken beyond repair by a 1-line fix.** Concrete errors:
- `cli/mod.rs:2167` — missing `use std::path::Path`
- `bench_harness/long_running/project.rs:208` — `ProjectStatus` missing `#[derive(Hash)]` → cascades to 10 × `HashMap::get` failures in `report.rs`
- `bench_harness/long_running/runner.rs:240` — `.parse()` needs turbofish
- `bench_harness/long_running/mod.rs:59` — the known `TaskSetup` re-export error

### 2. Live Endpoint Behavior (Agent B)

Confirmed live at `https://crazyshit.ngrok.io/v1`:
- Model: `Qwen3.5-122B-A10B-NVFP4-yarn-1010k`
- Backend: sglang
- Max context: 1,010,000 tokens

**Critical discovery — tool calling format:**
```json
"content": "<tool_call>\n<function=get_weather>\n<parameter=city>\nParis\n</parameter>\n</function>\n</tool_call>",
"tool_calls": [],
"finish_reason": "tool_calls"
```

The server sets `finish_reason: tool_calls` but leaves `tool_calls: []` and puts the XML in `content`. Any selfware profile talking to this endpoint **must** use `native_function_calling = false` plus a Qwen-style XML parser, not native JSON. Round 1 told the user to do the opposite.

**Thinking mode:**
- Default: ON, fills `reasoning_content` field
- Disable via `chat_template_kwargs: {"enable_thinking": false}` — confirmed working
- `reasoning_effort` field: accepted but not effective; use `enable_thinking` instead

**Latency baseline:**
- Small prompt (11 tokens): TTFT ~5.4s
- 10K context (12K tokens prefill): TTFT ~12.5s
- 10K context + thinking ON: ~23s for 10 completion tokens
- Prefill-dominated; streaming helps perceived latency

### 3. Unwrap Panic Risk Refined (Agent D)

| Metric | Count |
|--------|-------|
| Raw `.unwrap()` | 3,440 |
| Raw `.expect()` | 208 |
| **Total panic-risk** | **3,648** |
| Safe `unwrap_or_*` variants | 1,455 (miscounted by Round 1) |
| In test files | 606 |
| Production unwraps | ~3,042 |
| **Real HIGH-RISK sites on external data** | **~55** |

Critical points:
- **`agent/execution.rs`: 0 production unwraps** (148 are in test modules)
- **`agent/task_runner.rs`: 0 production unwraps** (101 tests)
- **`agent/tool_dispatch.rs`: 0 production unwraps** (20 tests)
- **`api/client.rs`: 0 unwraps at all**
- **`tools/file.rs`: 0 production unwraps** (136 tests)

The agent core reasoning loop — the thing that actually runs when you talk to the agent — is **unwrap-free in production code**.

Real high-risk sites are concentrated in:
1. `devops/process_manager.rs:1951` — `serde_json::from_str().unwrap()` on process log output
2. `computer/window.rs` — JSON parse on desktop automation output
3. `mcp/server.rs:43` — external protocol parsing (43 unwraps here)

**Severity downgrade: CRITICAL → MEDIUM.** Fix ~55 sites, not 3,450.

### 4. Git Churn & Instability Hotspots (Agent E)

- **440 commits in 30 days** (~103/week, ~14.7/day)
- **107 total branches**; ~90 are ephemeral `agent-YYYYMMDD-*` with no cleanup policy
- **30% of last 50 commits are `[AGENT CHECKPOINT]` noise**
- **31% of commits are automated** (Tre Bu Chet 218, Codex 20, Gemini 4, dependabot 27)
- **Ivo identity fragmented**: `Ivo Galic` (486) / `Ivo Galić` (98) / `IG` (14) — needs `.mailmap`

**Top 6 churned files (last 14 days) — all in `src/agent/`:**
| Rank | File | Commits |
|------|------|---------|
| 1 | `src/agent/mod.rs` | 19 |
| 2 | `src/agent/tool_dispatch.rs` | 16 |
| 3 | `src/agent/execution.rs` | 13 |
| 4 | `src/cli/args.rs` | 11 |
| 5 | `src/cli/mod.rs` | 10 |
| 6 | `src/agent/task_runner.rs` | 9 |

**The agent loop is the instability epicenter.** Matches the "harden/fix/recover" commit messages. Expect latent bugs here.

**Vaporware last-touched dates:**
| Module | Last Touched | Status |
|--------|--------------|--------|
| `supervision/mod.rs` | Feb 27 (41 days ago) | **Stale** |
| `self_healing.rs` | Mar 5 (35 days) | **Stale** |
| `browser/mod.rs` | Mar 26 (14 days) | Stale (cleanup only) |
| `evolution/daemon.rs` | Mar 26 (14 days) | Checkpoint only |
| `swl/mod.rs` | Mar 30 (10 days) | **Actively developed** (5 commits in 14 days) |

### 5. Vaporware Verification (Agent C)

| Claim | Verdict |
|-------|---------|
| Browser module all stubs | **VERIFIED** — 64 lines, methods log + return `Ok()` with dummy data, no Playwright/chromiumoxide/fantoccini |
| Supervision `restart_child`/`escalate` are no-ops | **VERIFIED** — `supervision/mod.rs:208-211` and `:247` only log, comments admit "In a real implementation, this would…" |
| SWL not wired to CLI | **REFUTED** — `cli/mod.rs:1354-1402` calls `lower_document()` then `executor.execute()` |
| Evolution never called | **REFUTED** — `cli/mod.rs:1084-1150` starts daemon with full `EvolutionConfig` |
| 3 dead files in src/ root (`autoscaler.rs`, `sampling.rs`, `cache.rs`) | **VERIFIED** — not declared in `lib.rs`, no `use crate::` references |
| `self_healing.rs` + `self_healing/` coexist | **VERIFIED** — both present, `lib.rs:158` declares `pub mod self_healing;` |
| 3,450 unwrap count | **VERIFIED** (but see Agent D for refinement) |

---

## Updated Module Health Matrix

Corrections against Round 1's ratings:

| Module | Round 1 | Round 2 | Change |
|--------|---------|---------|--------|
| `orchestration/coordinator` | Active MIXED | **DEAD** (no constructors called) | Downgrade |
| `orchestration/scratchpad` | Active MIXED | **DEAD** | Downgrade |
| `swl/` | Experimental (not wired) | **Active** (wired + 5 commits/14d) | Upgrade |
| `evolution/` | Stale (never called) | **Feature-gated Active** (daemon starts) | Upgrade |
| `agent/execution.rs` | Critical unwraps | **Production-safe** | Downgrade risk |
| `agent/task_runner.rs` | Critical unwraps | **Production-safe** | Downgrade risk |
| `bench_harness` | 1 compile error | **14 compile errors** | Upgrade severity |
| `selfware.toml` default | Endpoint dead | **Endpoint live** | Reverse |

---

## Revised Top Priorities

### Fix Now (Under 1 Hour Total)

1. **Delete 3 dead files** — `autoscaler.rs`, `sampling.rs`, `cache.rs` (20 KB, confirmed orphans)
2. **Resolve `self_healing` structural conflict** — move to `self_healing/mod.rs`
3. **Fix Ivo identity fragmentation** — add `.mailmap` with three aliases mapped to one identity
4. **Do NOT change `selfware.toml` endpoint** — it works
5. **Do NOT flip `native_function_calling` to true** — SGLang emits XML, current `false` is correct

### Fix This Session (1-2 Hours)

6. **Fix 14 bench-harness errors** — not just the TaskSetup issue:
   - Add `use std::path::Path` in `cli/mod.rs`
   - Add `#[derive(Hash)]` to `ProjectStatus` in `project.rs:208`
   - Fix turbofish in `runner.rs:240`
   - Fix TaskSetup re-export in `mod.rs:59`
7. **Delete dead orchestration code** — `coordinator.rs` + `scratchpad.rs` have zero constructors; eliminate them or document why they exist
8. **Document Qwen XML tool-call format** — and verify the parser handles `<tool_call><function=NAME><parameter=KEY>VALUE</parameter></function></tool_call>` correctly
9. **Add `chat_template_kwargs: {enable_thinking: false}`** to tool-calling / latency-critical configs

### Fix This Week (Medium-effort)

10. **Supervision restart/escalate** — still vaporware, still needs implementing (severity unchanged from Round 1)
11. **Browser automation** — still all stubs; either implement or remove browser tools from registry
12. **~55 high-risk unwraps on external data** — focus on `devops/process_manager.rs`, `mcp/server.rs`, `computer/window.rs`
13. **Config cleanup** — the 13-config deletion from Round 1 still stands, but re-verify the 5 "wrong native_function_calling" flag fixes use the **correct direction** (`false`, not `true`)
14. **Prune 90 ephemeral agent branches** older than 14 days
15. **Filter AGENT CHECKPOINT commits** — 30% noise in history; route to separate ref namespace or squash

---

## What Round 1 Got Right

- 103.8 GB test artifacts and disk-full pattern (uncontested)
- 62 markdown files in root clutter (uncontested)
- 13 unused config files identified correctly (same list, but for different reasons)
- `.gitignore` gaps (`.selfware/tool_results/`, `hello` binaries, `tarpaulin-report.html`)
- Browser module is stubs
- Supervision `restart_child`/`escalate` are no-ops
- `self_healing.rs` structural conflict is real
- Dead files in src/ root are real
- Documentation redundancy is real

## What Round 1 Got Wrong

- Endpoint is live (not dead)
- SWL is wired (not vaporware at the CLI level)
- Evolution is wired (not vaporware at the CLI level)
- Unwrap severity is overstated (CRITICAL → MEDIUM)
- `native_function_calling` direction is backwards (should be `false`, not `true`)
- bench-harness is much more broken than stated (14 errors not 1)
- Orchestration `coordinator`/`scratchpad` are DEAD, not "Active MIXED"

---

## Round 2 Confidence

| Area | Confidence | Why |
|------|-----------|-----|
| Build status | HIGH | Actually ran cargo |
| Endpoint behavior | HIGH | Actually curled it |
| Unwrap panic risk | HIGH | Per-file breakdown verified |
| Vaporware claims | HIGH | Read actual file contents |
| Git churn data | HIGH | Real git log output |
| Config cleanup list | MEDIUM | Reused from Round 1, direction flipped on native_function_calling |

*Audit-only. No changes applied to code, configs, or git state.*
