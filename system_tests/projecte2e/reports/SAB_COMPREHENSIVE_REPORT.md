# Selfware Agentic Benchmark Suite (SAB) - Final Report

## Executive Summary

**143 scenario runs across 12 rounds** of the Selfware Agentic Benchmark
Suite, testing Qwen/Qwen3-Coder-Next-FP8 (1M context) on 12 coding scenarios
ranging from easy bug fixes to expert-level concurrency debugging.

| Metric | Value |
|--------|-------|
| Model | **Qwen/Qwen3-Coder-Next-FP8** |
| Endpoint | `https://crazyshit.ngrok.io/v1` |
| Max Context | 1,010,000 tokens |
| Total Rounds | 12 |
| Total Scenario Runs | 143 |
| Grand Average | **85/100** |
| Steady-State Average (R2-R12) | **87/100** |
| Best Round | R2, R11: 96/100 |
| BLOOM Rounds (>=85) | 8/12 |
| Zero-FROST Rounds | 4/12 |
| S-Tier Scenarios (100% reliable) | 5/12 |

## Round-by-Round Results

| Round | Score | Rating | BLOOM | GROW | FROST | Passed | Note |
|-------|-------|--------|-------|------|-------|--------|------|
| R1 | 60/100 | 🌿 GROW | 6 | 1 | 4 | 7/11 | 12 parallel (overloaded) |
| R2 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 | 6 parallel |
| R3 | 70/100 | 🌿 GROW | 7 | 2 | 3 | 9/12 | 6 parallel |
| R4 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 | 6 parallel |
| R5 | 79/100 | 🌿 GROW | 8 | 2 | 2 | 10/12 | 6 parallel |
| R6 | 81/100 | 🌿 GROW | 9 | 1 | 2 | 10/12 | 6 parallel |
| R7 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 | 6 parallel |
| R8 | 89/100 | 🌸 BLOOM | 10 | 1 | 1 | 11/12 | 6 parallel |
| R9 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 | 6 parallel |
| R10 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 | 6 parallel |
| R11 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 | 6 parallel |
| R12 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 | 6 parallel |

## Scenario Reliability Matrix

| Scenario | Difficulty | R1 | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | Avg | Reliability | Tier |
|----------|-----------|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-----|-------------|------|
| `easy_calculator` | easy | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100% | S |
| `easy_string_ops` | easy | 100 | 100 | 100 | 90 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 99 | 100% | S |
| `medium_json_merge` | medium | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100% | S |
| `medium_bitset` | medium | 100 | 100 | 0 | 100 | 100 | 100 | 0 | 100 | 90 | 100 | 100 | 100 | 82 | 83% | A |
| `hard_scheduler` | hard | 0 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 91 | 92% | A |
| `hard_event_bus` | hard | 0 | 100 | 0 | 100 | 0 | 100 | 100 | 90 | 100 | 100 | 100 | 100 | 74 | 75% | B |
| `expert_async_race` | expert | 0 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 0 | 83 | 83% | A |
| `security_audit` | hard | 90 | 100 | 0 | 0 | 0 | 0 | 100 | 100 | 100 | 90 | 100 | 100 | 65 | 67% | B |
| `perf_optimization` | hard | - | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100% | S |
| `codegen_task_runner` | hard | 100 | 100 | 90 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 99 | 100% | S |
| `testgen_ringbuf` | medium | 80 | 80 | 80 | 80 | 70 | 80 | 70 | 80 | 70 | 80 | 80 | 80 | 77 | 75% | B |
| `refactor_monolith` | medium | 0 | 80 | 80 | 80 | 80 | 0 | 80 | 0 | 80 | 80 | 80 | 70 | 59 | 67% | B |

## Tier Classification

### Tier S: Rock Solid
*100% reliability, never fails*

- **`easy_calculator`** (easy) - avg 100/100, 100% reliable, ~51s avg
- **`easy_string_ops`** (easy) - avg 99/100, 100% reliable, ~114s avg
- **`medium_json_merge`** (medium) - avg 100/100, 100% reliable, ~105s avg
- **`perf_optimization`** (hard) - avg 100/100, 100% reliable, ~382s avg
- **`codegen_task_runner`** (hard) - avg 99/100, 100% reliable, ~152s avg

### Tier A: Highly Reliable
*>80% reliability, occasional misses*

- **`medium_bitset`** (medium) - avg 82/100, 83% reliable, ~142s avg
- **`hard_scheduler`** (hard) - avg 91/100, 92% reliable, ~93s avg
- **`expert_async_race`** (expert) - avg 83/100, 83% reliable, ~185s avg

### Tier B: Moderate
*50-80% reliability, needs monitoring*

- **`hard_event_bus`** (hard) - avg 74/100, 75% reliable, ~426s avg
- **`security_audit`** (hard) - avg 65/100, 67% reliable, ~385s avg
- **`testgen_ringbuf`** (medium) - avg 77/100, 75% reliable, ~306s avg
- **`refactor_monolith`** (medium) - avg 59/100, 67% reliable, ~366s avg

### Tier C: Unreliable
*<50% reliability, not production-ready*

*(none)*

## Failure Analysis

### Primary Failure Modes

1. **Repetition Loops** (most common): The model gets stuck repeating the same file_edit/file_write operation, consuming the entire timeout without making progress. Observed in security_audit, hard_scheduler (R1).
2. **Timeout Exhaustion**: Complex scenarios require more time than allocated. The agent makes correct progress but runs out of time. Observed in hard_event_bus, expert_async_race, refactor_monolith.
3. **Endpoint Overload**: R1 with 12 parallel jobs showed 60/100 avg. Reducing to 6 parallel improved to 87/100+ avg. Concurrency control is critical.
4. **Post-Validation Hangs**: Exponential algorithms in perf_optimization would hang `cargo test` indefinitely. Fixed by adding 120s timeout to post-validation.

### Scenario-Specific Issues

- **security_audit** (67% reliable): Agent correctly implements 3-4 of 5 vulnerability pairs but gets stuck in repetition loops on XSS escaping or info leak sanitization. When it works, it scores 90-100.
- **refactor_monolith** (67% reliable): Rust module restructuring is challenging. Agent sometimes gets confused by import paths after splitting files into modules.
- **testgen_ringbuf** (75% reliable): Consistently scores 70-80. The agent writes good tests but never achieves a clean exit (exit code 0), always losing 10-20 points.
- **hard_event_bus** (75% reliable): Complex Display trait implementation where format string must match exactly. Agent sometimes cannot find the right format.

## Performance Trends

The system shows clear improvement over rounds:
- **R1-R3**: Warm-up phase, avg 75/100 (affected by overload and configuration tuning)
- **R4-R8**: Stabilization, avg 83/100 (consistent 6-parallel, improved timeouts)
- **R9-R12**: Peak performance, avg 93/100 (four consecutive BLOOM rounds)

This trend suggests the model benefits from fresh work directories and consistent endpoint load.

## Conclusions

1. **Steady-state 87/100 average** demonstrates strong agentic coding capability across difficulty levels.
2. **5 S-tier scenarios** (easy_calculator, easy_string_ops, medium_json_merge, perf_optimization, codegen_task_runner) are completely reliable and production-ready.
3. **Concurrency management is critical**: 6 parallel is optimal for this endpoint. 12 parallel causes significant degradation.
4. **Repetition loops are the #1 failure mode**, not lack of coding ability. A repetition detector or diversity-promoting sampling could significantly improve reliability.
5. **The model handles expert-level tasks**: expert_async_race (concurrent race condition debugging) achieves 83% reliability with 83/100 avg, demonstrating real-world agentic coding capability.

