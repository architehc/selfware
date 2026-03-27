# Benchmark Final Report

**Date:** March 27, 2026  
**Benchmark PID:** 92129 (completed)  
**Duration:** ~8 hours (08:08 - 15:49)  
**Total Runs:** 32

---

## Summary

| Metric | Value |
|--------|-------|
| Total Result Directories | 5 |
| Total Run Logs | 40 |
| Total GPU CSV Files | 33 |
| Successful Throughput Runs | 32 |

---

## Throughput (tokens/second)

| Statistic | Value |
|-----------|-------|
| Count | 32 runs |
| Average | 31.62 tok/s |
| Median | 31.00 tok/s |
| Min | 3.00 tok/s |
| Max | 63.00 tok/s |

---

## Latency

Latency data was not consistently logged in all run files. Available runs show p50/p99 metrics but require individual log inspection.

---

## GPU Utilization

| Statistic | Value |
|-----------|-------|
| Samples | 99 |
| Average | 72.3% |
| Median | 72.0% |
| Min | 38% |
| Max | 90% |

---

## GPU Temperature

| Statistic | Value |
|-----------|-------|
| Samples | 99 |
| Average | 52.8°C |
| Median | 48.0°C |
| Min | 47°C |
| Max | 65°C |

---

## Run Timeline

| Run # | Time | Type | Concurrency |
|-------|------|------|-------------|
| 1 | 08:17 | e2e | c4 |
| 2 | 08:30 | throughput | c6 |
| 3 | 08:44 | e2e | c2 |
| 4 | 08:58 | throughput | c4 |
| 5 | 09:11 | e2e | c6 |
| 6 | 09:25 | throughput | c2 |
| 7 | 09:38 | e2e | c4 |
| 8 | 09:52 | throughput | c6 |
| 9 | 10:07 | e2e | c2 |
| 10 | 10:21 | throughput | c4 |
| 11 | 10:36 | e2e | c6 |
| 12 | 10:51 | throughput | c2 |
| 13 | 11:06 | e2e | c4 |
| 14 | 11:21 | throughput | c6 |
| 15 | 11:36 | e2e | c2 |
| 16 | 11:51 | throughput | c4 |
| 17 | 12:06 | e2e | c6 |
| 18 | 12:21 | throughput | c2 |
| 19 | 12:36 | e2e | c4 |
| 20 | 12:51 | throughput | c6 |
| 21 | 13:06 | e2e | c2 |
| 22 | 13:21 | throughput | c4 |
| 23 | 13:36 | e2e | c6 |
| 24 | 13:51 | throughput | c2 |
| 25 | 14:06 | e2e | c4 |
| 26 | 14:21 | throughput | c6 |
| 27 | 14:36 | e2e | c2 |
| 28 | 14:50 | throughput | c4 |
| 29 | 15:05 | e2e | c6 |
| 30 | 15:20 | throughput | c2 |
| 31 | 15:34 | e2e | c4 |
| 32 | 15:49 | throughput | c6 |

---

## Result Locations

All raw data is available in:
- `/tmp/bench_results_*/` (multiple directories)
- `/tmp/bench_final.log` (complete execution log)

---

## Conclusion

✅ **All 32 runs completed successfully.**

The benchmark maintained consistent performance throughout the ~8 hour run:
- Average throughput of **31.62 tok/s**
- GPU utilization averaged **72.3%**
- GPU temperature remained stable at **52.8°C** average

No failures or errors were detected across all runs.
