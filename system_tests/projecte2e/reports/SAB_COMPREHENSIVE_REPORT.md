# Selfware Agentic Benchmark Suite (SAB) - Comprehensive Report

## Overview

- **Model**: Qwen/Qwen3-Coder-Next-FP8
- **Endpoint**: https://crazyshit.ngrok.io/v1
- **Max Context**: 1,010,000 tokens
- **Total Rounds**: 8 (R1 with 12 parallel, R2-R8 with 6 parallel)
- **Total Scenario Runs**: 95
- **Date**: 2026-03-01

## Key Metrics

| Metric | Value |
|--------|-------|
| Grand Average (all rounds) | **81/100** |
| Steady-State Average (R2-R8) | **84/100** |
| Best Round | R2: 96/100 BLOOM |
| Worst Round (excl. R1 overload) | R3: 70/100 GROW |
| Scenarios at 100% reliability | 5/12 |

## Round-by-Round Results

| Round | Avg Score | Rating | BLOOM | GROW | FROST | Passed | Note |
|-------|-----------|--------|-------|------|-------|--------|------|
| R1 | 60/100 | 🌿 GROW | 6 | 1 | 4 | 7/11 | 12 parallel (overloaded) |
| R2 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 | 6 parallel |
| R3 | 70/100 | 🌿 GROW | 7 | 2 | 3 | 9/12 | 6 parallel |
| R4 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 | 6 parallel |
| R5 | 79/100 | 🌿 GROW | 8 | 2 | 2 | 10/12 | 6 parallel |
| R6 | 81/100 | 🌿 GROW | 9 | 1 | 2 | 10/12 | 6 parallel |
| R7 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 | 6 parallel |
| R8 | 89/100 | 🌸 BLOOM | 10 | 1 | 1 | 11/12 | 6 parallel |

## Per-Scenario Reliability

| Scenario | Difficulty | Scores (R1→R8) | Avg | Min | Reliability |
|----------|-----------|----------------|-----|-----|-------------|
| `easy_calculator` | easy | 100, 100, 100, 100, 100, 100, 100, 100 | 100 | 100 | 100% |
| `easy_string_ops` | easy | 100, 100, 100, 90, 100, 100, 100, 100 | 98 | 90 | 100% |
| `medium_json_merge` | medium | 100, 100, 100, 100, 100, 100, 100, 100 | 100 | 100 | 100% |
| `medium_bitset` | medium | 100, 100, 0, 100, 100, 100, 0, 100 | 75 | 0 | 75% |
| `hard_scheduler` | hard | 0, 100, 100, 100, 100, 100, 100, 100 | 87 | 0 | 88% |
| `hard_event_bus` | hard | 0, 100, 0, 100, 0, 100, 100, 90 | 61 | 0 | 62% |
| `expert_async_race` | expert | 0, 100, 100, 100, 100, 100, 100, 100 | 87 | 0 | 88% |
| `security_audit` | hard | 90, 100, 0, 0, 0, 0, 100, 100 | 48 | 0 | 50% |
| `perf_optimization` | hard | 100, 100, 100, 100, 100, 100, 100 | 100 | 100 | 100% |
| `codegen_task_runner` | hard | 100, 100, 90, 100, 100, 100, 100, 100 | 98 | 90 | 100% |
| `testgen_ringbuf` | medium | 80, 80, 80, 80, 70, 80, 70, 80 | 77 | 70 | 75% |
| `refactor_monolith` | medium | 0, 80, 80, 80, 80, 0, 80, 0 | 50 | 0 | 62% |

## Tier Classification

### Tier S - Rock Solid (100% reliability, avg >= 95)

- **`easy_calculator`** (easy): avg 100/100, 100% reliability
- **`easy_string_ops`** (easy): avg 98/100, 100% reliability
- **`medium_json_merge`** (medium): avg 100/100, 100% reliability
- **`perf_optimization`** (hard): avg 100/100, 100% reliability
- **`codegen_task_runner`** (hard): avg 98/100, 100% reliability

### Tier A - Highly Reliable (>80% reliability)

- **`hard_scheduler`** (hard): avg 87/100, 88% reliability
- **`expert_async_race`** (expert): avg 87/100, 88% reliability

### Tier B - Moderate (50-80% reliability)

- **`medium_bitset`** (medium): avg 75/100, 75% reliability
- **`hard_event_bus`** (hard): avg 61/100, 62% reliability
- **`security_audit`** (hard): avg 48/100, 50% reliability
- **`testgen_ringbuf`** (medium): avg 77/100, 75% reliability
- **`refactor_monolith`** (medium): avg 50/100, 62% reliability

### Tier C - Unreliable (<50% reliability)


## Failure Analysis

### Common Failure Modes

1. **Repetition Loops**: The model sometimes gets stuck repeating the same file_edit/file_write operation, consuming the entire timeout. Observed in: hard_scheduler (R1), security_audit (R3-R5)
2. **Timeout Before Completion**: Complex scenarios (hard_event_bus, expert_async_race) sometimes need more than the allocated time. The agent makes correct progress but runs out of time.
3. **Endpoint Overload**: R1 with 12 parallel jobs showed degraded performance. Reducing to 6 parallel improved results significantly.
4. **Post-Validation Hangs**: The perf_optimization LCS test (exponential recursion) would hang indefinitely during post-validation if the agent didnt fix it. Fixed by adding 120s timeout to post-validation.

### Scenario-Specific Issues

- **security_audit**: Agent correctly implements 3-4 of 5 vulnerability pairs but gets stuck in a repetition loop on the remaining 1-2, typically on XSS escaping or info leak sanitization.
- **refactor_monolith**: Agent sometimes struggles with Rust module restructuring, getting confused by import paths after splitting files.
- **hard_event_bus**: Complex Display trait implementation where the agent struggles to match the exact expected format string.
- **testgen_ringbuf**: Consistently scores 70-80/100. The agent writes good tests but never achieves a clean exit (exit code 0), losing 10 points every time.

## Conclusions

1. **The system reliably handles easy-to-hard coding tasks** with 5 out of 12 scenarios at 100% reliability.
2. **Steady-state average of 84/100** (excluding overloaded R1) demonstrates solid agentic coding capability.
3. **Concurrency matters**: 6 parallel is the sweet spot for this endpoint. 12 parallel causes degradation.
4. **Repetition loops are the #1 failure mode**, not lack of coding ability. The model knows what to do but sometimes gets stuck repeating the same action.
5. **Complex multi-file refactoring (refactor_monolith) and security-specific tasks (security_audit) are the weakest areas**, suggesting these need more targeted prompting or longer timeouts.

