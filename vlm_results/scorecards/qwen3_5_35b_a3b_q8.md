# VLM Scorecard: Qwen3.5-35B-A3B (Q8)

**Model**: `qwen/qwen3.5-35b-a3b`
**Hardware**: Apple M2 Max (96GB RAM, LM Studio)
**Quantization**: Q8_0 (8-bit GGUF)
**Context**: 262K tokens
**Concurrency**: 1
**Date**: 2026-03-06

## Overall

| Metric | Value |
|--------|-------|
| **Overall Score** | **76%** |
| **Overall Rating** | **BLOOM** |
| Total Tokens | 60,489 |
| Total Duration | 3,186s (53 min) |
| Avg Latency/Scenario | 161.6s |
| Scenarios | 19 (2 timed out) |

## Level Breakdown

| Level | Difficulty | Score | Rating | Pass Threshold | Tokens | Avg Latency |
|-------|-----------|-------|--------|---------------|--------|-------------|
| L1 TUI State | Easy | **50%** | WILT | 80% | 12,151 | 276.1s |
| L2 Diagnostics | Medium | **67%** | GROW | 70% | 1,312 | 210.4s |
| L3 Architecture | Hard | **63%** | BLOOM | 60% | 16,694 | 183.4s |
| L4 Profiling | Very Hard | **100%** | BLOOM | 50% | 12,576 | 130.4s |
| L5 Layout | Extreme | **100%** | BLOOM | 40% | 10,259 | 104.5s |
| Mega Evolution | Mega | **76%** | BLOOM | 50% | 7,497 | 65.2s |

## Per-Scenario Detail

### L1 TUI State (50% WILT)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Dashboard Normal | 0% | FROST | _(none)_ | 7,179 | 335.0s |
| Dashboard Error | 100% | BLOOM | error, true | 3,021 | 123.5s |
| Help Panel | 0% | FROST | _(timeout)_ | 0 | 600.2s |
| Loading State | 100% | BLOOM | loading, true | 1,951 | 45.6s |

### L2 Diagnostics (67% GROW)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Lifetime Error (E0106) | 100% | BLOOM | error_type | 734 | 18.2s |
| Type Mismatch (E0308) | 100% | BLOOM | type, mismatch, expected | 578 | 12.7s |
| Trait Bound (E0277) | 0% | FROST | _(timeout)_ | 0 | 600.1s |

### L3 Architecture (63% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Evolution Engine | 40% | WILT | fitness, mutation | 2,793 | 76.1s |
| Agent Pipeline | 75% | BLOOM | agent, tool, context | 10,867 | 392.1s |
| Safety Layers | 75% | BLOOM | safety, validation, path | 3,034 | 82.1s |

### L4 Profiling (100% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Simple Flamegraph | 100% | BLOOM | function, hot | 5,398 | 175.9s |
| Multithread Profile | 100% | BLOOM | thread, function | 3,926 | 121.1s |
| Memory Profile | 100% | BLOOM | allocat, memory | 3,252 | 94.2s |

### L5 Layout (100% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Simple Split | 100% | BLOOM | layout, horizontal, constraint, percentage | 5,295 | 177.8s |
| Dashboard Grid | 100% | BLOOM | vertical, horizontal, header, sidebar | 2,473 | 68.6s |
| Complex Nested | 100% | BLOOM | tab, list, chart, popup, nested | 2,491 | 67.2s |

### Mega Evolution (76% BLOOM)

| Scenario | Score | Rating | Tokens | Latency |
|----------|-------|--------|--------|---------|
| Iteration Scoring | 28% | WILT | 2,045 | 48.6s |
| Progression Analysis | 100% | BLOOM | 2,517 | 69.0s |
| Rating Prediction | 100% | BLOOM | 2,935 | 78.0s |

**Mega dimension accuracy** (iteration_01 scoring):

| Dimension | Accuracy | Notes |
|-----------|----------|-------|
| Composition | 0.30 | ~70pts off ground truth |
| Hierarchy | 0.20 | ~80pts off ground truth |
| Readability | 0.10 | ~90pts off ground truth |
| Consistency | 0.94 | Very accurate (~6pts off) |
| Accessibility | 0.94 | Very accurate (~6pts off) |

## Three-Way Comparison

| Level | Qwen3.5-9B Q8 | Qwen3-VL-30B FP8 | Qwen3.5-35B Q8 |
|-------|---------------|-------------------|-----------------|
| L1 TUI State | **88%** BLOOM | 62% GROW | 50% WILT |
| L2 Diagnostics | 39% WILT | 67% GROW | **67%** GROW |
| L3 Architecture | 30% WILT | 47% GROW | **63%** BLOOM |
| L4 Profiling | **100%** BLOOM | **100%** BLOOM | **100%** BLOOM |
| L5 Layout | 85% BLOOM | **100%** BLOOM | **100%** BLOOM |
| Mega Evolution | 53% BLOOM | 33% WILT | **76%** BLOOM |
| **Overall** | 66% BLOOM | 68% BLOOM | **76%** BLOOM |
| Tokens | 17,976 | 38,656 | 60,489 |
| Duration | 118s | 486s | 3,186s |
| Hardware | RTX 4090 | 2x RTX 4090 | M2 Max 96GB |

## Strengths

- **Highest overall score** (76%) across all tested models
- **Best architecture comprehension**: 63% BLOOM on L3 — first model to pass the Hard threshold (60%)
- **Perfect on L4 + L5**: Joins the 30B model in achieving 100% on profiling and layout
- **Best Mega score** (76%): Only model to correctly use garden-themed ratings (BLOOM/GROW) in rating prediction
- **Lifetime error detection**: First model to score 100% on E0106 — correctly identifies error type from block-pixel render
- **Deep reasoning**: Chain-of-thought produces thorough analysis with context identification (agent pipeline: 75%, safety layers: 75%)

## Weaknesses

- **Very slow**: 53 minutes total, avg 161s per scenario (~27x slower than the 9B model)
- **Timeout prone**: 2/19 scenarios timed out at 600s (help_panel, trait_bound) — thinking loops can run away
- **Token hungry**: 60,489 tokens total (3.4x the 9B model) due to extensive reasoning
- **Dashboard Normal blind spot**: 0% with 7,179 tokens — overthinks and fails to emit "active_panel" or "theme" keywords
- **Dimension scoring asymmetry**: Very accurate on consistency/accessibility but wildly off on composition/hierarchy/readability

## Notes

- Images generated with `text_to_png()` — block-pixel rendering (no real font glyphs), white on black
- Image sizes: 640-800px wide, 2-3MB uncompressed PNG per fixture
- Temperature: 0.2 (near-deterministic)
- Inline thinking model (reasoning woven into response, no `<think>` tags)
- Running on Apple M2 Max with 96GB unified memory via LM Studio
- Q8_0 quantization, 262K context window
- 2 scenarios hit the 600s timeout — would likely score higher with longer timeouts
- Without timeouts, estimated true score would be ~82-85% (help_panel and trait_bound both likely to pass)
