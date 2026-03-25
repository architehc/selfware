# Benchmark Report — qwen3.5-27b
**Date**: 2026-03-25T14:13:04.387114247+00:00 | **Endpoint**: http://localhost:8000/v1 | **Concurrency**: 16

## Summary

| Metric | Value |
|--------|-------|
| Tasks | 12/20 passed (40% error rate) |
| Avg Score | 90.5% |
| Throughput | 6 tok/s |
| Tokens | 11,993 prompt + 2,480 completion |
| Duration | 426.1s |

## Latency

| Percentile | ms |
|-----------|----|
| p50 | 244616 |
| p95 | 300002 |
| p99 | 300002 |
| avg | 219200 |
| min/max | 83285/300002 |

## Per-Task Results

| Task | Status | Score | Latency | Tokens |
|------|--------|-------|---------|--------|
| astropy__astropy-12907 | FAIL | N/A | 300002ms | 0+0 |
| astropy__astropy-14182 | FAIL | N/A | 300001ms | 0+0 |
| astropy__astropy-14365 | FAIL | N/A | 300001ms | 0+0 |
| astropy__astropy-14995 | PASS | 75% | 105728ms | 2477+138 |
| astropy__astropy-6938 | PASS | 100% | 85352ms | 410+142 |
| astropy__astropy-7746 | PASS | 100% | 236689ms | 834+208 |
| django__django-10914 | FAIL | N/A | 300002ms | 0+0 |
| django__django-10924 | PASS | 100% | 182412ms | 1777+243 |
| django__django-11001 | PASS | 100% | 265992ms | 1110+315 |
| django__django-11019 | FAIL | N/A | 300001ms | 0+0 |
| django__django-11039 | PASS | 60% | 178411ms | 539+239 |
| django__django-11049 | PASS | 100% | 178412ms | 363+239 |
| django__django-11099 | PASS | 100% | 244616ms | 359+295 |
| django__django-11133 | PASS | 100% | 126125ms | 542+202 |
| django__django-11179 | PASS | 100% | 83285ms | 541+134 |
| django__django-11283 | FAIL | N/A | 300001ms | 0+0 |
| django__django-11422 | PASS | 71% | 135646ms | 866+186 |
| django__django-11564 | FAIL | N/A | 300001ms | 0+0 |
| django__django-11583 | PASS | 80% | 161330ms | 2175+139 |
| django__django-11620 | FAIL | N/A | 300002ms | 0+0 |
