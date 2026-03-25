# Benchmark Report — qwen3.5-27b
**Date**: 2026-03-25T03:32:45.926464121+00:00 | **Endpoint**: http://localhost:8000/v1 | **Concurrency**: 32

## Summary

| Metric | Value |
|--------|-------|
| Tasks | 5/32 passed (84% error rate) |
| Avg Score | 14.6% |
| Throughput | 148 tok/s |
| Tokens | 1,228 prompt + 16,297 completion |
| Duration | 110.0s |

## Latency

| Percentile | ms |
|-----------|----|
| p50 | 56102 |
| p95 | 97164 |
| p99 | 110043 |
| avg | 62427 |
| min/max | 27524/110043 |

## Per-Task Results

| Task | Status | Score | Latency | Tokens |
|------|--------|-------|---------|--------|
| task-00 | FAIL | 0% | 27524ms | 37+512 |
| task-01 | FAIL | 0% | 78836ms | 37+512 |
| task-02 | FAIL | 0% | 110042ms | 40+512 |
| task-03 | FAIL | 0% | 27524ms | 38+512 |
| task-04 | FAIL | 0% | 27568ms | 38+512 |
| task-05 | FAIL | 0% | 27524ms | 36+512 |
| task-06 | PASS | 100% | 65754ms | 42+473 |
| task-07 | FAIL | 0% | 27524ms | 39+512 |
| task-08 | PASS | 67% | 36699ms | 37+512 |
| task-09 | FAIL | 0% | 91273ms | 37+512 |
| task-10 | FAIL | 0% | 110043ms | 40+512 |
| task-11 | FAIL | 0% | 56102ms | 38+512 |
| task-12 | FAIL | 0% | 56101ms | 38+512 |
| task-13 | FAIL | 0% | 27524ms | 36+512 |
| task-14 | FAIL | 0% | 81014ms | 42+512 |
| task-15 | FAIL | 0% | 42448ms | 39+512 |
| task-16 | FAIL | 0% | 56101ms | 37+512 |
| task-17 | FAIL | 0% | 91273ms | 37+512 |
| task-18 | PASS | 100% | 88490ms | 40+488 |
| task-19 | FAIL | 0% | 56102ms | 38+512 |
| task-20 | FAIL | 0% | 56156ms | 38+512 |
| task-21 | FAIL | 0% | 42448ms | 36+512 |
| task-22 | FAIL | 0% | 81014ms | 42+512 |
| task-23 | FAIL | 0% | 89763ms | 39+512 |
| task-24 | FAIL | 0% | 81013ms | 37+512 |
| task-25 | FAIL | 0% | 42448ms | 37+512 |
| task-26 | PASS | 100% | 91273ms | 40+512 |
| task-27 | FAIL | 0% | 42448ms | 38+512 |
| task-28 | PASS | 100% | 41106ms | 38+488 |
| task-29 | FAIL | 0% | 56101ms | 36+512 |
| task-30 | FAIL | 0% | 91272ms | 42+512 |
| task-31 | FAIL | 0% | 97164ms | 39+512 |
