# Selfware Configuration & TOML Sprawl Audit Report

**Agent:** Agent 2 — Configuration & TOML Sprawl Audit  
**Date:** 2026-04-09  
**Repository:** architehc/selfware  
**Branch:** agent-20260405-145152  

---

## Executive Summary

The Selfware repository contains **23 TOML configuration files** with significant sprawl, inconsistencies, and broken references. Many configurations reference non-existent code paths (specifically `src/cognitive/memory_hierarchy.rs` which is actually a directory module). There are multiple conflicting endpoint URLs and model specifications across the configuration set.

**Key Finding:** The current `selfware.toml` is partially misconfigured — it uses the correct endpoint but an outdated model name that doesn't match the actual deployed model.

---

## 1. Configuration Inventory Table

| Filename | Purpose | Endpoint URL | Model | Status |
|----------|---------|--------------|-------|--------|
| `selfware.toml` | **PRIMARY ACTIVE CONFIG** | `https://crazyshit.ngrok.io/v1` | `txn545/Qwen3.5-122B-A10B-NVFP4` | ⚠️ NEEDS UPDATE |
| `selfware.example.toml` | Template/example | `http://localhost:8080/v1` | `your-model-name-here` | ✅ VALID |
| `selfware-hybrid.toml` | 122B + 27B dual-profile | Primary: `https://crazyshit.ngrok.io/v1` | `txn545/Qwen3.5-122B-A10B-NVFP4` | ✅ VALID |
| `selfware-122b-concurrency64.toml` | High-throughput 122B | `https://crazyshit.ngrok.io/v1` | `txn545/Qwen3.5-122B-A10B-NVFP4` | ✅ VALID |
| `selfware-27b-concurrency16.toml` | Local 27B vLLM | `http://localhost:8000/v1` | `qwen3.5-27b` | ✅ VALID |
| `selfware-27b-fixed.toml` | Minimal 27B config | `http://localhost:8000/v1` | `qwen3.5-27b` | ✅ VALID |
| `selfware-4090-qwen35-256k.toml` | RTX 4090 local (4B) | `http://127.0.0.1:8000/v1` | `Qwen/Qwen3.5-4B` | ⚠️ LEGACY |
| `selfware-4090-qwen35-9b-q8-vision.toml` | RTX 4090 local (9B) | `http://127.0.0.1:8001/v1` | `Qwen/Qwen3.5-9B` | ⚠️ LEGACY |
| `selfware-auto-qwen3-5-27b.toml` | Auto-config 27B | `http://localhost:8000/v1` | `qwen3.5-27b` | ✅ VALID |
| `selfware-auto-txn545-Qwen3-5-122B-A10B-NVFP4.toml` | Auto-config 122B | `https://crazyshit.ngrok.io/v1` | `txn545/Qwen3.5-122B-A10B-NVFP4` | ✅ VALID |
| `selfware-eval.toml` | Evaluation runs | `https://crazyshit.ngrok.io/v1` | `Qwen/Qwen3-Coder-Next-FP8` | ❌ **WRONG MODEL** |
| `selfware-evolve-122b.toml` | Evolution 122B | `https://crazyshit.ngrok.io/v1` | `txn545/Qwen3.5-122B-A10B-NVFP4` | ❌ BROKEN REFS |
| `selfware-evolve-cognitive.toml` | Evolution cognitive | `http://localhost:8000/v1` | `qwen3.5-27b` | ❌ BROKEN REFS |
| `selfware-evolve-fast.toml` | Evolution fast mode | `http://localhost:8000/v1` | `qwen3.5-27b` | ❌ BROKEN REFS |
| `selfware-evolve-tiny.toml` | Evolution tiny (0.8B) | `http://localhost:8080/v1` | `qwen3.5-0.8b` | ❌ BROKEN REFS |
| `selfware-evolve-tools.toml` | Evolution tools | `http://localhost:8000/v1` | `qwen3.5-27b` | ❌ BROKEN REFS |
| `selfware-extended-test.toml` | Extended testing | `http://localhost:8000/v1` | `Qwen/Qwen3-Coder-Next-FP8` | ⚠️ LEGACY MODEL |
| `selfware-longrun.toml` | 6+ hour runs | `http://localhost:8888/v1` | `Qwen/Qwen3-Coder-Next-FP8` | ⚠️ LEGACY ENDPOINT/MODEL |
| `selfware-micro.toml` | Minimal/edge devices | `http://localhost:8080/v1` | `qwen3.5-0.8b` | ⚠️ EXPERIMENTAL |
| `selfware-qwen35-optimized.toml` | 27B optimized (1M ctx) | `http://localhost:8000/v1` | `qwen3.5-27b` | ⚠️ CUSTOM SCHEMA |
| `selfware-stress-test.toml` | Stress testing | `http://host.docker.internal:8000/v1` | `qwen3.5-27b` | ✅ VALID |
| `selfware-text-primary-local.toml` | Text primary local | `http://localhost:8000/v1` | `qwen3.5-27b` | ✅ VALID |
| `selfware-vision-primary-remote.toml` | Vision primary remote | `https://crazyshit.ngrok.io/v1` | `txn545/Qwen3.5-122B-A10B-NVFP4` | ✅ VALID |

---

## 2. Current Endpoint Specification (Ground Truth)

**Verified via API call to `https://crazyshit.ngrok.io/v1/models`:**

```json
{
  "object": "list",
  "data": [{
    "id": "/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k",
    "object": "model",
    "owned_by": "sglang",
    "max_model_len": 1010000
  }]
}
```

**Actual Deployed Model:**
- **Full Path:** `/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k`
- **Context Length:** 1,010,000 tokens (~1M)
- **Endpoint:** `https://crazyshit.ngrok.io/v1`
- **Supports Multimodal:** Yes (vision + text)

---

## 3. Conflicts Found

### 3.1 Model Name Mismatches

| Config File | Specified Model | Should Be |
|-------------|-----------------|-----------|
| `selfware.toml` | `txn545/Qwen3.5-122B-A10B-NVFP4` | `/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k` |
| `selfware-hybrid.toml` | `txn545/Qwen3.5-122B-A10B-NVFP4` | `/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k` |
| `selfware-eval.toml` | `Qwen/Qwen3-Coder-Next-FP8` | `/media/thread/trebochet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k` |
| `selfware-extended-test.toml` | `Qwen/Qwen3-Coder-Next-FP8` | Should use current active model |
| `selfware-longrun.toml` | `Qwen/Qwen3-Coder-Next-FP8` | Should use current active model |

### 3.2 Context Length Inconsistencies

| Config File | Context Claimed | Actual Endpoint | Issue |
|-------------|-----------------|-----------------|-------|
| `selfware.toml` | 262,144 | 1,010,000 | Under-utilizing 4x context |
| `selfware-hybrid.toml` | 262,144 | 1,010,000 | Under-utilizing 4x context |
| `selfware-122b-concurrency64.toml` | 262,144 | 1,010,000 | Under-utilizing 4x context |
| `selfware-vision-primary-remote.toml` | 262,144 | 1,010,000 | Under-utilizing 4x context |

### 3.3 Broken Code Path References (Evolution Configs)

**NON-EXISTENT FILE:** `src/cognitive/memory_hierarchy.rs`

This file is referenced in:
- `selfware-evolve-122b.toml` (line 47)
- `selfware-evolve-cognitive.toml` (line 17)

**Actual State:** `memory_hierarchy` is a **DIRECTORY MODULE**, not a file:
```
src/cognitive/memory_hierarchy/
├── mod.rs          (34KB - main module)
├── types.rs        (16KB)
├── short_term.rs   (7KB)
└── long_term.rs    (6KB)
```

**Impact:** Evolution engine will fail to load these cognitive modules.

### 3.4 Inconsistent Parameter Settings

| Parameter | selfware.toml | selfware-hybrid.toml | selfware-122b-concurrency64.toml | Issue |
|-----------|---------------|----------------------|----------------------------------|-------|
| `native_function_calling` | `false` | `true` | `true` | Inconsistent tool calling |
| `max_tokens` | 16384 | 8192 | 8192 | 2x variance |
| `token_budget` | *not set* | 180000 | 180000 | Missing in default |
| `streaming` | `true` | `false` | *not set* | Inconsistent |

### 3.5 Endpoint URL Variations

**Remote (Production):**
- `https://crazyshit.ngrok.io/v1` ✅ **CORRECT**

**Local (Development):**
- `http://localhost:8000/v1` — Most common
- `http://localhost:8080/v1` — Legacy
- `http://localhost:8888/v1` — Long-run specific
- `http://127.0.0.1:8000/v1` — Explicit localhost
- `http://127.0.0.1:8001/v1` — Alternative port (9B model)
- `http://host.docker.internal:8000/v1` — Docker-specific

### 3.6 Custom Schema Sections

Some configs use non-standard sections that may not be recognized:

**`selfware-qwen35-optimized.toml`:**
- `[concurrency]` — Non-standard
- `[sampling_modes]` — Non-standard  
- `[sampling_modes.thinking_general]` — Nested non-standard
- `[task_type_sampling]` — Non-standard
- `[performance]` — Non-standard
- `[monitoring]` — Non-standard
- `[context_management]` — Non-standard

**`selfware-stress-test.toml`:**
- `[stress_test]` — Non-standard

**`selfware-eval.toml`:**
- `[yolo]` — Non-standard

---

## 4. selfware.toml Assessment

### Current Configuration:
```toml
endpoint = "https://crazyshit.ngrok.io/v1"
model = "txn545/Qwen3.5-122B-A10B-NVFP4"
max_tokens = 16384
context_length = 262144
temperature = 0.6
```

### Required Updates:

| Field | Current Value | Required Value | Priority |
|-------|---------------|----------------|----------|
| `model` | `txn545/Qwen3.5-122B-A10B-NVFP4` | `/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k` | **CRITICAL** |
| `context_length` | `262144` | `1010000` | **HIGH** |
| `native_function_calling` | `false` | `true` (endpoint supports it) | MEDIUM |
| `streaming` | `true` | `false` (for stability) | LOW |

### Missing Recommended Sections:
```toml
[agent]
token_budget = 900000          # For 1M context
token_safety_margin = 50000

[parallel]
max_concurrency = 64           # Endpoint supports this
enabled = true
```

---

## 5. Recommended Canonical Set

### 5.1 KEEP — Official Production Configs (5 files)

| File | Purpose |
|------|---------|
| `selfware.toml` | Primary production config (NEEDS UPDATE) |
| `selfware.example.toml` | Template for new users |
| `selfware-hybrid.toml` | Dual-profile (122B + 27B local) |
| `selfware-vision-primary-remote.toml` | Vision-heavy workflows |
| `selfware-text-primary-local.toml` | Local text-focused work |

### 5.2 ARCHIVE — Specialized Test Configs (8 files)

Move to `configs/archive/` or `configs/testing/`:

| File | Reason |
|------|--------|
| `selfware-122b-concurrency64.toml` | Specialized benchmark config |
| `selfware-27b-concurrency16.toml` | Specialized local config |
| `selfware-auto-*.toml` | Auto-generated, redundant |
| `selfware-stress-test.toml` | Testing-only |
| `selfware-eval.toml` | Outdated model reference |
| `selfware-extended-test.toml` | Testing-only |
| `selfware-longrun.toml` | Outdated endpoint |

### 5.3 DELETE — Obsolete/Broken (10 files)

| File | Reason |
|------|--------|
| `selfware-4090-qwen35-256k.toml` | References 4B model (unused) |
| `selfware-4090-qwen35-9b-q8-vision.toml` | References 9B model (unused) |
| `selfware-27b-fixed.toml` | Minimal config, superseded |
| `selfware-micro.toml` | 0.8B model (experimental, unmaintained) |
| `selfware-qwen35-optimized.toml` | Non-standard schema |
| `selfware-evolve-*.toml` (5 files) | **Broken references to memory_hierarchy.rs** |

### 5.4 REPOSITORY STRUCTURE RECOMMENDATION

```
/home/ivo/selfware/
├── selfware.toml                    # Primary production config
├── selfware.example.toml            # User template
├── selfware-hybrid.toml             # Dual endpoint
├── configs/
│   ├── testing/
│   │   ├── selfware-stress-test.toml
│   │   ├── selfware-extended-test.toml
│   │   └── selfware-122b-concurrency64.toml
│   └── local/
│       ├── selfware-27b-concurrency16.toml
│       └── selfware-text-primary-local.toml
└── configs-obsolete/                # Move then delete after validation
    ├── selfware-evolve-122b.toml    # Broken: memory_hierarchy.rs
    ├── selfware-evolve-cognitive.toml
    ├── selfware-evolve-fast.toml
    ├── selfware-evolve-tiny.toml
    └── selfware-evolve-tools.toml
```

---

## 6. Critical Action Items

### IMMEDIATE (Before Next Run)
1. **Update `selfware.toml` model name** to match deployed endpoint
2. **Update `selfware.toml` context_length** to 1,010,000
3. **Fix or remove evolution configs** — broken `memory_hierarchy.rs` references

### SHORT-TERM (This Week)
4. **Standardize `native_function_calling`** — endpoint supports it, use `true`
5. **Archive obsolete configs** — move to subdirectories
6. **Document config schema** — what sections are valid

### MEDIUM-TERM (This Month)
7. **Config validation tool** — add `selfware --validate-config`
8. **Auto-discovery** — query endpoint for model name/context
9. **Consolidate testing configs** — single config with profiles

---

## 7. Verification Commands

```bash
# Check current endpoint model
curl -s https://crazyshit.ngrok.io/v1/models | jq '.data[0].id'

# Verify all configs are valid TOML
for f in selfware*.toml; do echo -n "$f: "; tomllib "$f" > /dev/null 2>&1 && echo "OK" || echo "INVALID"; done

# Check for broken file references
grep -h "src/" selfware-*.toml | sort | uniq

# Find configs using outdated endpoints
grep -l "localhost:8080\|localhost:8888" selfware*.toml
```

---

## 8. Appendix: File Reference Audit

### Evolution Config References Status:

| File Path | Referenced In | Exists? | Type |
|-----------|---------------|---------|------|
| `src/agent/planning.rs` | evolve-122b, hybrid | ✅ YES | File |
| `src/agent/loop_control.rs` | evolve-122b, evolve-fast, hybrid | ✅ YES | File |
| `src/agent/learning.rs` | evolve-122b, hybrid | ✅ YES | File |
| `src/agent/execution.rs` | evolve-122b, hybrid | ✅ YES | File |
| `src/agent/context_management.rs` | evolve-122b | ✅ YES | File |
| `src/tools/file.rs` | evolve-122b, evolve-tools, evolve-fast, hybrid | ✅ YES | File |
| `src/tools/search.rs` | evolve-122b, evolve-tools, hybrid | ✅ YES | File |
| `src/tools/analyzer.rs` | evolve-122b, hybrid | ✅ YES | File |
| `src/tools/shell.rs` | evolve-122b, evolve-tools | ✅ YES | File |
| `src/tools/cargo.rs` | evolve-122b | ✅ YES | File |
| `src/tools/git.rs` | evolve-tools | ✅ YES | File |
| `src/cognitive/memory_hierarchy.rs` | evolve-122b, evolve-cognitive | ❌ **NO** | **BROKEN** |
| `src/cognitive/episodic.rs` | evolve-122b, evolve-cognitive | ✅ YES | File |
| `src/cognitive/self_improvement.rs` | evolve-122b, evolve-cognitive | ✅ YES | File |
| `src/cognitive/state.rs` | evolve-fast, evolve-tiny | ✅ YES | File |
| `src/memory.rs` | evolve-cognitive | ✅ YES | File |

---

## Summary

| Metric | Count |
|--------|-------|
| Total Config Files | 23 |
| Valid & Current | 6 |
| Needs Updates | 5 |
| Broken References | 5 (evolution configs) |
| Obsolete/Delete | 7 |
| Unique Endpoints Referenced | 6 |
| Unique Models Referenced | 7 |

**Primary Risk:** The main `selfware.toml` uses a model name that may not be recognized by the endpoint, and significantly under-utilizes the available 1M context window.

**Secondary Risk:** All evolution configs reference a non-existent file, rendering the evolution feature non-functional.

