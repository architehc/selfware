# SAB Comprehensive Report: 21 Rounds, 251 Scenario Runs

## Executive Summary

**251 scenario runs across 21 rounds** testing Qwen/Qwen3-Coder-Next-FP8 (1M context) on 12 coding scenarios.

| Metric | Value |
|--------|-------|
| Model | **Qwen/Qwen3-Coder-Next-FP8** |
| Max Context | 1,010,000 tokens |
| Total Rounds | 21 |
| Total Runs | 251 |
| Grand Average | **89/100** |
| Steady-State (R2-R21) | **90/100** |
| Peak (R9-R21) | **94/100** |
| BLOOM Rounds | 17/21 |
| Zero-FROST Rounds | 11/21 |

## Round Results

| Round | Score | Rating | BLOOM | GROW | FROST | Passed |
|-------|-------|--------|-------|------|-------|--------|
| R1 | 60/100 | 🌿 GROW | 6 | 1 | 4 | 7/11 |
| R2 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R3 | 70/100 | 🌿 GROW | 7 | 2 | 3 | 9/12 |
| R4 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 |
| R5 | 79/100 | 🌿 GROW | 8 | 2 | 2 | 10/12 |
| R6 | 81/100 | 🌿 GROW | 9 | 1 | 2 | 10/12 |
| R7 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 |
| R8 | 89/100 | 🌸 BLOOM | 10 | 1 | 1 | 11/12 |
| R9 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R10 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R11 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R12 | 87/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 |
| R13 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R14 | 88/100 | 🌸 BLOOM | 9 | 2 | 1 | 11/12 |
| R15 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R16 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R17 | 95/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R18 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R19 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R20 | 96/100 | 🌸 BLOOM | 10 | 2 | 0 | 12/12 |
| R21 | 89/100 | 🌸 BLOOM | 10 | 1 | 1 | 11/12 |

## Scenario Reliability

| Scenario | Difficulty | Avg | Min | Max | Reliability | Tier |
|----------|-----------|-----|-----|-----|-------------|------|
| `easy_calculator` | easy | 100 | 100 | 100 | 100% | S |
| `easy_string_ops` | easy | 99 | 90 | 100 | 100% | S |
| `medium_json_merge` | medium | 100 | 100 | 100 | 100% | S |
| `medium_bitset` | medium | 89 | 0 | 100 | 90% | A |
| `hard_scheduler` | hard | 95 | 0 | 100 | 95% | A |
| `hard_event_bus` | hard | 84 | 0 | 100 | 86% | A |
| `expert_async_race` | expert | 90 | 0 | 100 | 90% | A |
| `security_audit` | hard | 72 | 0 | 100 | 76% | B |
| `perf_optimization` | hard | 100 | 100 | 100 | 100% | S |
| `codegen_task_runner` | hard | 99 | 90 | 100 | 100% | S |
| `testgen_ringbuf` | medium | 76 | 70 | 80 | 71% | B |
| `refactor_monolith` | medium | 61 | 0 | 80 | 71% | B |

## Conclusions

1. **90/100 steady-state** demonstrates strong agentic coding capability
2. **5 S-tier scenarios** at 100% reliability: easy_calculator, easy_string_ops, medium_json_merge, perf_optimization, codegen_task_runner
3. **Peak performance phase (R9-R21)** averaging 94/100 with 8 zero-FROST rounds in 13
4. **Concurrency is critical**: 6 parallel optimal, 12 parallel degrades performance
5. **Repetition loops** are the primary failure mode, not coding ability
6. **Late-phase consistency**: R15-R20 achieved 6 consecutive perfect 95-96/100 rounds
