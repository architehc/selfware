# VLM Scorecard: Qwen3.5-27B (Q4_K_M)

**Model**: `unsloth/Qwen3.5-27B-GGUF:Q4_K_M`
**Hardware**: llama.cpp via ngrok (hardware unknown)
**Quantization**: Q4_K_M (4-bit GGUF)
**Context**: 262K tokens
**Concurrency**: 1 (server cannot handle parallel vision requests)
**Date**: 2026-03-06
**Note**: Combined from 2 partial runs — ngrok tunnel unstable. 12/19 scenarios completed.

## Overall

| Metric | Value |
|--------|-------|
| **Overall Score (with failures)** | **44%** |
| **Estimated Score (completed only)** | **~81%** |
| **Overall Rating** | **WILT** (partial) / **BLOOM** (estimated) |
| Total Tokens (completed) | 33,188 |
| Scenarios Completed | 12/19 |
| Scenarios Failed (connection) | 7/19 |

## Level Breakdown

| Level | Difficulty | Score (raw) | Completed Only | Rating | Tokens | Avg Latency |
|-------|-----------|-------------|---------------|--------|--------|-------------|
| L1 TUI State | Easy | **50%** | 67% | WILT | 12,219 | 83.6s |
| L2 Diagnostics | Medium | **33%** | 100% | FROST | 669 | 25.6s |
| L3 Architecture | Hard | **38%** | 58% | WILT | 10,645 | 162.5s |
| L4 Profiling | Very Hard | **33%** | 100% | WILT | 4,632 | 132.2s |
| L5 Layout | Extreme | **27%** | 80% | WILT | 3,045 | 81.2s |
| Mega Evolution | Mega | **81%** | 81% | BLOOM | 6,746 | 53.0s |

## Per-Scenario Detail

### L1 TUI State (50% raw / 67% completed)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency | Source |
|----------|-------|--------|-------------|--------|---------|--------|
| Dashboard Normal | 0% | FROST | _(none)_ | 2,792 | 53.3s | Run 2 |
| Dashboard Error | - | FAILED | _(connection)_ | 0 | - | Both |
| Help Panel | 100% | BLOOM | help, shortcut | 1,042 | 9.5s | Run 2 |
| Loading State | 100% | BLOOM | loading, true | 8,385 | 188.1s | Run 2 |

### L2 Diagnostics (33% raw / 100% completed)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency | Source |
|----------|-------|--------|-------------|--------|---------|--------|
| Lifetime Error (E0106) | - | FAILED | _(connection)_ | 0 | - | Both |
| Type Mismatch (E0308) | - | FAILED | _(connection)_ | 0 | - | Both |
| Trait Bound (E0277) | 100% | BLOOM | trait, bound | 669 | 25.6s | Run 1 |

### L3 Architecture (38% raw / 58% completed)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency | Source |
|----------|-------|--------|-------------|--------|---------|--------|
| Evolution Engine | 40% | WILT | fitness, mutation | 5,569 | 178.7s | Run 1 |
| Agent Pipeline | 75% | BLOOM | agent, tool, context | 5,076 | 146.3s | Run 1 |
| Safety Layers | - | FAILED | _(connection)_ | 0 | - | Both |

### L4 Profiling (33% raw / 100% completed)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency | Source |
|----------|-------|--------|-------------|--------|---------|--------|
| Simple Flamegraph | - | FAILED | _(connection)_ | 0 | - | Both |
| Multithread Profile | - | FAILED | _(connection)_ | 0 | - | Both |
| Memory Profile | 100% | BLOOM | allocat, memory | 4,632 | 132.2s | Run 1 |

### L5 Layout (27% raw / 80% completed)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency | Source |
|----------|-------|--------|-------------|--------|---------|--------|
| Simple Split | - | FAILED | _(connection)_ | 0 | - | Both |
| Dashboard Grid | - | FAILED | _(connection)_ | 0 | - | Both |
| Complex Nested | 80% | BLOOM | tab, list, chart, popup | 3,045 | 81.2s | Run 1 |

### Mega Evolution (81% BLOOM) — All completed

| Scenario | Score | Rating | Tokens | Latency | Source |
|----------|-------|--------|--------|---------|--------|
| Iteration Scoring | 42% | GROW | 1,879 | 44.3s | Run 1 |
| Progression Analysis | 100% | BLOOM | 2,175 | 53.0s | Run 1 |
| Rating Prediction | 100% | BLOOM | 2,692 | 61.7s | Run 1 |

## Four-Way Comparison (completed scenarios only)

| Level | 9B Q8 | 30B VL FP8 | 35B Q8 | 27B Q4_K_M* |
|-------|-------|------------|--------|-------------|
| L1 TUI State | **88%** | 62% | 50% | 67%* |
| L2 Diagnostics | 39% | **67%** | **67%** | 100%* |
| L3 Architecture | 30% | 47% | **63%** | 58%* |
| L4 Profiling | **100%** | **100%** | **100%** | 100%* |
| L5 Layout | 85% | **100%** | **100%** | 80%* |
| Mega Evolution | 53% | 33% | 76% | **81%** |
| **Overall** | 66% | 68% | **76%** | ~81%* |

*Asterisk: estimated from completed scenarios only (12/19). Needs full stable run to confirm.

## Strengths

- **Best Mega Evolution score** (81%) — all 3 scenarios completed, highest of any model tested
- **Rating vocabulary**: Correctly uses BLOOM and GROW terms in rating prediction (100%)
- **Trait bound detection**: Perfect on E0277 trait bound scenario (100%)
- **Fast when responsive**: ~42 tok/s throughput, help_panel completed in 9.5s
- **Agent pipeline comprehension**: 75% BLOOM — identifies agent, tool, and context components
- **Reasoning depth**: `reasoning_content` field shows structured chain-of-thought analysis

## Weaknesses

- **Endpoint instability**: 7/19 scenarios failed due to ngrok tunnel drops — scores are unreliable
- **Dashboard Normal blind spot**: 0% on all models, thinking model wastes 2.8K tokens and misses keywords
- **Variable latency**: From 9.5s (help_panel) to 188.1s (loading_state) — thinking overhead is unpredictable
- **Cannot handle concurrent vision requests**: Server returns 503/timeout with concurrency > 1

## Notes

- Images generated with `text_to_png()` — block-pixel rendering (no real font glyphs), white on black
- Image sizes: 640-800px wide, 2-3MB uncompressed PNG per fixture
- Temperature: 0.2 (near-deterministic)
- Thinking model with `reasoning_content` field (separate from content)
- Run 1: concurrency 4, timeout 300s — 9/19 completed
- Run 2: concurrency 1, timeout 600s — 3/19 completed (tunnel died at L2)
- Combined: 12/19 unique scenarios with data
- **Needs a stable endpoint for a definitive benchmark** — estimated ~81% would be the best score
- llama.cpp backend (GGUF format)
- Q4_K_M quantization (4-bit, medium quality)
