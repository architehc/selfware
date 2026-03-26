# Benchmark Report — qwen3.5-27b
**Date**: 2026-03-26T12:22:40.846439464+00:00 | **Endpoint**: http://localhost:8000/v1 | **Concurrency**: 12

## Summary

| Metric | Value |
|--------|-------|
| Tasks | 12/12 passed (0% error rate) |
| Avg Score | 97.2% |
| Throughput | 141 tok/s |
| Tokens | 813 prompt + 10,068 completion |
| Duration | 71.6s |

## Latency

| Percentile | ms |
|-----------|----|
| p50 | 34880 |
| p95 | 66756 |
| p99 | 71566 |
| avg | 38978 |
| min/max | 22820/71566 |

## Per-Task Results

| Task | Status | Score | Latency | Tokens |
|------|--------|-------|---------|--------|
| go-concurrent-map | PASS | 100% | 34880ms | 80+729 |
| go-http-middleware | PASS | 100% | 46440ms | 68+1016 |
| go-worker-pool | PASS | 67% | 32336ms | 75+668 |
| js-debounce | PASS | 100% | 27412ms | 54+552 |
| js-promise-pool | PASS | 100% | 71566ms | 65+1669 |
| python-async-queue | PASS | 100% | 66756ms | 64+1537 |
| python-decorator | PASS | 100% | 39380ms | 58+837 |
| python-merge-sort | PASS | 100% | 25092ms | 55+501 |
| rust-binary-search | PASS | 100% | 28192ms | 70+571 |
| rust-fibonacci | PASS | 100% | 22820ms | 60+451 |
| rust-lru-cache | PASS | 100% | 42494ms | 85+916 |
| ts-type-safe-event | PASS | 100% | 30370ms | 79+621 |
