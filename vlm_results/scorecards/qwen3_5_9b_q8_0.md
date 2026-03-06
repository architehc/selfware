# VLM Scorecard: Qwen 3.5-9B (Q8_0)

**Model**: `unsloth/Qwen3.5-9B-GGUF:Q8_0`
**Hardware**: NVIDIA RTX 4090 (single GPU)
**Quantization**: Q8_0 (8-bit GGUF)
**Context**: 262K tokens
**Concurrency**: 1
**Date**: 2026-03-06

## Overall

| Metric | Value |
|--------|-------|
| **Overall Score** | **66%** |
| **Overall Rating** | **BLOOM** |
| Total Tokens | 17,976 |
| Total Duration | 118.4s |
| Avg Latency/Scenario | 6.2s |
| Scenarios | 19 |

## Level Breakdown

| Level | Difficulty | Score | Rating | Pass Threshold | Tokens | Avg Latency |
|-------|-----------|-------|--------|---------------|--------|-------------|
| L1 TUI State | Easy | **88%** | BLOOM | 80% | 3,527 | 5.0s |
| L2 Diagnostics | Medium | **39%** | WILT | 70% | 948 | 2.0s |
| L3 Architecture | Hard | **30%** | WILT | 60% | 3,686 | 8.4s |
| L4 Profiling | Very Hard | **100%** | BLOOM | 50% | 2,494 | 4.3s |
| L5 Layout | Extreme | **85%** | BLOOM | 40% | 4,232 | 12.0s |
| Mega Evolution | Mega | **53%** | BLOOM | 50% | 3,089 | 6.1s |

## Per-Scenario Detail

### L1 TUI State (88% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Dashboard Normal | 50% | WILT | theme | 773 | 3.9s |
| Dashboard Error | 100% | BLOOM | error, true | 760 | 3.8s |
| Help Panel | 100% | BLOOM | help, shortcut | 1,154 | 7.9s |
| Loading State | 100% | BLOOM | loading, true | 840 | 4.4s |

### L2 Diagnostics (39% WILT)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Lifetime Error (E0106) | 0% | FROST | _(none)_ | 301 | 1.8s |
| Type Mismatch (E0308) | 67% | GROW | type, expected | 314 | 2.1s |
| Trait Bound (E0277) | 50% | WILT | trait | 333 | 1.9s |

### L3 Architecture (30% WILT)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Evolution Engine | 40% | WILT | fitness, mutation | 1,585 | 13.0s |
| Agent Pipeline | 25% | FROST | agent | 1,091 | 6.6s |
| Safety Layers | 25% | FROST | validation | 1,010 | 5.5s |

### L4 Profiling (100% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Simple Flamegraph | 100% | BLOOM | function, hot | 862 | 5.0s |
| Multithread Profile | 100% | BLOOM | thread, function | 772 | 3.4s |
| Memory Profile | 100% | BLOOM | allocat, memory | 860 | 4.6s |

### L5 Layout (85% BLOOM)

| Scenario | Score | Rating | Keywords Hit | Tokens | Latency |
|----------|-------|--------|-------------|--------|---------|
| Simple Split | 100% | BLOOM | layout, horizontal, constraint, percentage | 1,425 | 13.1s |
| Dashboard Grid | 75% | BLOOM | vertical, header, sidebar | 1,348 | 11.6s |
| Complex Nested | 80% | BLOOM | tab, list, chart, popup | 1,459 | 11.2s |

### Mega Evolution (53% BLOOM)

| Scenario | Score | Rating | Tokens | Latency |
|----------|-------|--------|--------|---------|
| Iteration Scoring | 59% | BLOOM | 981 | 5.5s |
| Progression Analysis | 100% | BLOOM | 976 | 6.8s |
| Rating Prediction | 0% | FROST | 1,132 | 5.9s |

**Mega dimension correlation** (iteration_01 vs ground truth):

| Dimension | Predicted | Ground Truth | Diff |
|-----------|-----------|-------------|------|
| Composition | 65 | 75 | -10 |
| Hierarchy | 70 | 70 | 0 |
| Readability | 60 | 80 | -20 |
| Consistency | 54 | 72 | -18 |
| Accessibility | 41 | 68 | -27 |

## Strengths

- **Layout understanding**: Strong at identifying widget types, layout directions, and constraints from ASCII mockups (85% on Extreme)
- **Generic concept recognition**: Perfect on profiling scenarios — reliably mentions functions, threads, memory, allocation
- **Error state detection**: Correctly identifies error/loading states in TUI screenshots (100% on 3/4 L1 scenarios)
- **Progression analysis**: Perfect score comparing before/after TUI iterations

## Weaknesses

- **Text extraction from blocky renders**: Cannot read specific text content (error codes, module names) from the block-pixel character rendering
- **Architecture diagram parsing**: Struggles to identify specific component names from ASCII box-and-arrow diagrams (30%)
- **Rating vocabulary**: Does not use the garden-themed rating terms (BLOOM/GROW) without stronger prompting
- **Dimension scoring accuracy**: Tends to underestimate accessibility and consistency dimensions

## Notes

- Images generated with `text_to_png()` — block-pixel rendering (no real font glyphs), white on black
- Image sizes: 640-800px wide, 2-3MB uncompressed PNG per fixture
- Temperature: 0.2 (near-deterministic)
- Scores stable across 2 runs (within +/-8% per level)
