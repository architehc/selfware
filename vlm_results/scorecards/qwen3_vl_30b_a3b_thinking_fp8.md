# VLM Scorecard: Qwen3-VL-30B-A3B Thinking (FP8)

**Model**: `Qwen/Qwen3-VL-30B-A3B-Thinking-FP8`
**Hardware**: 2x NVIDIA RTX 4090 (vLLM/sglang)
**Quantization**: FP8
**Context**: 131K tokens
**Concurrency**: 1
**Date**: 2026-03-06

## Overall

| Metric | Value |
|--------|-------|
| **Overall Score** | **68%** |
| **Overall Rating** | **BLOOM** |
| Total Tokens | 38,656 |
| Total Duration | 485.8s |
| Avg Latency/Scenario | 24.2s |
| Scenarios | 19 |

## Level Breakdown

| Level | Difficulty | Score | Rating | Pass Threshold | Tokens | Avg Latency |
|-------|-----------|-------|--------|---------------|--------|-------------|
| L1 TUI State | Easy | **62%** | GROW | 80% | 7,349 | 49.3s |
| L2 Diagnostics | Medium | **67%** | GROW | 70% | 3,273 | 10.9s |
| L3 Architecture | Hard | **47%** | GROW | 60% | 8,609 | 27.7s |
| L4 Profiling | Very Hard | **100%** | BLOOM | 50% | 5,366 | 16.2s |
| L5 Layout | Extreme | **100%** | BLOOM | 40% | 8,999 | 28.0s |
| Mega Evolution | Mega | **33%** | WILT | 50% | 5,060 | 13.3s |

## Per-Scenario Detail

### L1 TUI State (62% GROW)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Dashboard Normal | 0% | FROST | _(none)_ | 3,177 | 162.7s |
| Dashboard Error | 100% | BLOOM | error, true | 1,406 | 12.1s |
| Help Panel | 100% | BLOOM | help, shortcut | 994 | 7.0s |
| Loading State | 50% | WILT | loading | 1,772 | 15.3s |

### L2 Diagnostics (67% GROW)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Lifetime Error (E0106) | 0% | FROST | _(none)_ | 1,136 | 11.8s |
| Type Mismatch (E0308) | 100% | BLOOM | type, mismatch, expected | 1,000 | 10.1s |
| Trait Bound (E0277) | 100% | BLOOM | trait, bound | 1,137 | 11.0s |

### L3 Architecture (47% GROW)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Evolution Engine | 40% | WILT | fitness, mutation | 3,014 | 29.4s |
| Agent Pipeline | 50% | GROW | agent, tool | 2,514 | 23.3s |
| Safety Layers | 50% | GROW | safety, path | 3,081 | 30.5s |

### L4 Profiling (100% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Simple Flamegraph | 100% | BLOOM | function, hot | 1,856 | 16.1s |
| Multithread Profile | 100% | BLOOM | thread, function | 1,994 | 19.0s |
| Memory Profile | 100% | BLOOM | allocat, memory | 1,516 | 13.5s |

### L5 Layout (100% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Simple Split | 100% | BLOOM | layout, horizontal, constraint, percentage | 4,789 | 48.0s |
| Dashboard Grid | 100% | BLOOM | vertical, horizontal, header, sidebar | 1,968 | 17.1s |
| Complex Nested | 100% | BLOOM | tab, list, chart, popup, nested | 2,242 | 19.0s |

### Mega Evolution (33% WILT)

| Scenario | Score | Rating | Tokens | Latency |
|----------|-------|--------|--------|---------|
| Iteration Scoring | 0% | FROST | 2,106 | 16.9s |
| Progression Analysis | 100% | BLOOM | 1,570 | 12.9s |
| Rating Prediction | 0% | FROST | 1,384 | 10.1s |

## Head-to-Head vs Qwen3.5-9B Q8_0

| Level | Qwen3.5-9B Q8_0 | Qwen3-VL-30B FP8 | Delta |
|-------|-----------------|-------------------|-------|
| L1 TUI State | 88% BLOOM | 62% GROW | -26% |
| L2 Diagnostics | 39% WILT | 67% GROW | +28% |
| L3 Architecture | 30% WILT | 47% GROW | +17% |
| L4 Profiling | 100% BLOOM | 100% BLOOM | 0% |
| L5 Layout | 85% BLOOM | 100% BLOOM | +15% |
| Mega Evolution | 53% BLOOM | 33% WILT | -20% |
| **Overall** | **66% BLOOM** | **68% BLOOM** | **+2%** |
| Tokens | 17,976 | 38,656 | +115% |
| Duration | 118.4s | 485.8s | +310% |

## Strengths

- **Perfect layout comprehension**: 100% on all L5 layout scenarios — correctly identifies widget types, layout directions, constraints, percentages, and nested structures
- **Perfect profiling analysis**: 100% on all L4 profiling scenarios — reliably identifies functions, threads, memory allocation patterns
- **Improved diagnostics**: Successfully extracts "mismatch" and "bound" keywords that the 9B model missed (67% vs 39%)
- **Better architecture parsing**: Identifies more component names from ASCII diagrams (47% vs 30%), including "tool" and "safety"/"path" keywords
- **Chain-of-thought reasoning**: Thinking process helps with complex layout and architecture analysis

## Weaknesses

- **Thinking overhead hurts simple tasks**: Dashboard Normal took 162.7s and produced 3,177 tokens but scored 0% — overthinks simple screenshots
- **Verbose output**: 2.15x more tokens than the 9B model for the same 19 scenarios
- **Slow inference**: 4.1x slower overall despite dual 4090s (thinking tokens dominate)
- **Mega evolution regression**: Thinking format interferes with structured dimension scoring (0% on iteration scoring)
- **Rating vocabulary**: Still does not use garden-themed rating terms (BLOOM/GROW) without stronger prompting
- **Lifetime error blind spot**: Both models fail to identify E0106 lifetime errors from block-pixel renders

## Notes

- Images generated with `text_to_png()` — block-pixel rendering (no real font glyphs), white on black
- Image sizes: 640-800px wide, 2-3MB uncompressed PNG per fixture
- Temperature: 0.2 (near-deterministic)
- Model outputs `<think>...</think>` reasoning blocks before the answer
- First run with concurrency 4 failed (timeouts) — model is too slow for parallel vision requests
- vLLM/sglang backend on dual 4090s
- FP8 quantization (8-bit floating point)
