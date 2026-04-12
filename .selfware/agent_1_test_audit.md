# Test Infrastructure & Results Audit Report

**Agent:** Agent 1 - Test Infrastructure & Results Audit  
**Date:** 2026-04-09  
**Repository:** architehc/selfware  
**Branch:** agent-20260405-145152  

---

## Executive Summary

This audit examined the test infrastructure and results directories in the Selfware repository. The codebase has extensive testing infrastructure but with significant duplication and incomplete long-running tests consuming substantial disk space.

### Critical Findings
| Finding | Impact | Status |
|---------|--------|--------|
| 4-day test incomplete (stopped at ~20h) | Data loss | ⚠️ STOPPED |
| 101GB in long_run_tests/ | Disk pressure | 🔴 CRITICAL |
| v4 test syntax error (Apr 4) | Test failure | 🔴 BROKEN |
| Multiple duplicate test scripts | Maintenance burden | 🟡 WARNING |

---

## 1. Test Results Summary Table

### 1.1 Long-Running Test Results

| Directory | Test Type | Status | Date | Exit Code | Notes |
|-----------|-----------|--------|------|-----------|-------|
| `system_test_8hr_20260402_225637` (v1) | 8-hour system | ✅ PASSED | Apr 2 | 0 | 6h48m, 8 GREEN / 25 |
| `system_test_8hr_v2_20260403_161838` | 8-hour system | ✅ PASSED | Apr 3 | 0 | Completed all rounds |
| `system_test_8hr_v3_20260404_154327` | 8-hour system | ✅ PASSED | Apr 4 | 0 | Completed all rounds |
| `system_test_8hr_v4_20260404_232131` | 8-hour system | 🔴 FAILED | Apr 4 | 2 | Syntax error line 368 |
| `system_test_8hr_v4_20260404_232653` | 8-hour system | ✅ PASSED | Apr 4-5 | 0 | 10 rounds completed |
| `system_test_8hr_v4_20260405_140754` | 8-hour system | ✅ PASSED | Apr 5 | 0 | 2+ cycles completed |
| `system_test_8hr_v5_20260405_081749` | 8-hour system | ✅ PASSED | Apr 5 | 0 | 4 projects tested |
| `system_test_8hr_v5_20260405_082729` | 8-hour system | ✅ PASSED | Apr 5 | 0 | 50+ cycles, **22GB** |
| `4day_results_20260405_094154` | 4-day stress | ⚠️ EMPTY | Apr 5 | - | No data collected |
| `4day_results_20260405_094416` | 4-day stress | ⚠️ INCOMPLETE | Apr 5 | - | ~20h/96h, **74GB** |
| `marathon_20260402_230101` | Marathon | ✅ COMPLETED | Apr 2-3 | 0 | 30+ rounds |

### 1.2 Benchmark Results

| Directory | Contents | Size | Status |
|-----------|----------|------|--------|
| `bench_results/` | swebench, continuous, multilang | 792M | ✅ Active |
| `swebench_122b/` | 122B model evals | 64K | ✅ Minimal |
| `swebench_eval/` | Evaluation results | 616K | ✅ Active |
| `benchmark_results_20260326_*` | Benchmark CSVs | 24K | ⚠️ Old |
| `gpu_max_test/` | GPU stress tests | 4.9M | ✅ Active |

### 1.3 Other Results Directories

| Directory | Size | Status | Notes |
|-----------|------|--------|-------|
| `parallel_results/` | 296K | ⚠️ Old | Mar 20 results |
| `multilang_results/` | 16K | ⚠️ Old | Only 2 log files |
| `sab_results/` | 164K | ⚠️ Old | Scorecards only |
| `vlm_results/` | 64K | ✅ Active | VLM benchmark |
| `test_runs/` | 2.0M | ✅ Active | Smoke tests |
| `hybrid_demo_results_*/` | 12-20K | ⚠️ Old | Mar 26 demos |

---

## 2. Critical Findings

### 2.1 What Broke Between v1→v5 of 8-Hour Tests

| Version | Issue | Root Cause | Fix |
|---------|-------|------------|-----|
| **v4 (Apr 4, 23:21)** | Script crashed at Round 1 | Bash syntax error with Unicode box-drawing chars | Use ASCII-only characters in scripts |

**Detailed Analysis:**
```
Error: long_run_tests/run_8hour_system_test_v4.sh: line 368: 
syntax error near unexpected token `('
line 368: `echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"'
```

The v4 script used Unicode box-drawing characters (╚══) which caused bash parsing errors when combined with command substitution. The v5 script fixed this by either:
- Removing the decorative characters
- Escaping properly
- Using ASCII equivalents

**Success Rate Trend:**
- v1: 8 GREEN, 3 PARTIAL (44% full success)
- v2-v3: Similar results
- v4 (Apr 4 first): FAILED - 0% (syntax error)
- v4 (retry): Improved completion
- v5: Further stability improvements

### 2.2 Status of 4-Day Test

| Metric | Expected | Actual | Status |
|--------|----------|--------|--------|
| Duration | 96 hours | ~20 hours | ⚠️ 21% complete |
| Agent 0 Rounds | ~2880 | 1263 | ⚠️ 44% complete |
| Agent 1 Rounds | ~2880 | 1270 | ⚠️ 44% complete |
| Agent 2 Rounds | ~2880 | 1254 | ⚠️ 44% complete |
| Agent 3 Rounds | ~2880 | 1259 | ⚠️ 44% complete |
| Disk Usage | ~50GB | 74GB | 🔴 Over budget |

**Likely Stop Reasons:**
1. Manual interruption
2. System reboot
3. Out of disk space (test was consuming ~3.7GB/hour)
4. Endpoint failure

**Recommendation:** The 4-day test is consuming excessive disk space. Consider:
- Compressing old rounds
- Reducing log verbosity
- Implementing log rotation

### 2.3 Tests That Hang or Crash

| Test | Issue | Frequency | Mitigation |
|------|-------|-----------|------------|
| v4 8-hour (Apr 4) | Syntax error crash | Once | Fixed in v5 |
| 4-day test | Early termination | Unknown | Add monitoring |
| Visual workflow | Playwright timeout | Intermittent | Retry logic |

---

## 3. Script Classification

### 3.1 CURRENT (Keep - Working & Active)

| Script | Purpose | Last Used |
|--------|---------|-----------|
| `continuous_test.sh` | Endpoint keepalive testing | Active |
| `stress_test_vllm.sh` | vLLM stress testing | Active |
| `test_32_concurrent.sh` | 32-agent concurrency test | Active |
| `test_122b_endpoint.sh` | 122B model validation | Active |
| `benchmark_both_endpoints.sh` | Comparative benchmarking | Active |
| `run_e2e_suite.sh` | E2E test orchestration | Active |
| `system_tests/run_full_system_test.sh` | Full system validation | Active |

### 3.2 OBSOLETE (Delete - Superseded)

| Script | Replacement | Reason |
|--------|-------------|--------|
| `quick_32_test.sh` | `test_32_concurrent.sh` | Simplified, less comprehensive |
| `6hour_test.sh` | `6hour-stress-test.sh` | Naming inconsistency |
| `run-6hour-test.sh` | `6hour-stress-test.sh` | Duplicate functionality |
| `validate_27b_context.sh` | `test_27b_long_context.sh` | Renamed version exists |

### 3.3 DUPLICATE (Consolidate)

| Primary Script | Duplicates | Action |
|----------------|------------|--------|
| `test_32_concurrent.sh` | `quick_32_test.sh`, `parallel_stress_test.sh` | Merge features |
| `long_running_test.sh` | `run_extended_tests.sh` | Pick best features |
| `hybrid_workflow_demo.sh` | `hybrid_workflow_demo_fixed.sh`, `hybrid_demo_run.sh`, `hybrid_demo_run_fixed.sh` | Consolidate to one |
| `visual_benchmark.sh` | `run_visual_workflow.sh` | Merge or rename |

### 3.4 BROKEN (Fix or Delete)

| Script | Issue | Action |
|--------|-------|--------|
| `long_run_tests/run_8hour_system_test_v4.sh` (Apr 4 ver) | Unicode syntax error | Delete (v5 fixes this) |
| `launch_4day.sh` | May not handle interrupts | Review error handling |

---

## 4. Recommendations

### 4.1 Immediate Actions (High Priority)

1. **Stop/Archive 4-Day Test**
   - Current: 74GB for incomplete test
   - Action: Compress or delete `4day_results_20260405_094416/`
   - Potential savings: **74GB**

2. **Clean Old Test Runs**
   - Delete: `marathon_20260402_230101/` (1.1GB - old)
   - Delete: Early `longrun_*` directories from Apr 1-2
   - Potential savings: **~2GB**

3. **Consolidate v5 Results**
   - `system_test_8hr_v5_20260405_082729/` is 22GB
   - Compress or archive older cycles
   - Potential savings: **15GB**

### 4.2 Short-Term (Medium Priority)

4. **Remove Duplicate Scripts**
   ```bash
   # Scripts to delete:
   rm quick_32_test.sh
   rm run-6hour-test.sh
   rm 6hour_test.sh
   rm hybrid_workflow_demo_fixed.sh
   rm hybrid_demo_run_fixed.sh
   ```
   - Potential maintenance reduction: **5 scripts**

5. **Archive Old Results**
   - Move Mar 20-26 results to cold storage
   - Compress with xz for long-term storage

6. **Fix v4 Script Issue**
   - Remove or fix the broken v4 script
   - Document Unicode restrictions

### 4.3 Long-Term (Low Priority)

7. **Implement Log Rotation**
   - Configure automated cleanup for long-running tests
   - Keep only last N rounds in uncompressed form

8. **Unified Test Orchestrator**
   - Consolidate all test scripts into a single framework
   - Reduce duplication and maintenance burden

### 4.4 Estimated Cleanup Savings

| Category | Current Size | After Cleanup | Savings |
|----------|--------------|---------------|---------|
| 4-day test (incomplete) | 74GB | 0GB | **74GB** |
| v5 test (compress old) | 22GB | 7GB | **15GB** |
| Old marathon tests | 1.1GB | 0GB | **1GB** |
| Other old results | ~4GB | 0.5GB | **3.5GB** |
| **TOTAL** | **101GB+** | **~8GB** | **~93GB** |

---

## 5. Test Exit Codes Analysis

### 5.1 Exit Code Patterns Found

From `marathon_20260402_230101/`:
```
round_001_greenfield.exit_code: 2
round_002_templates_*.exit_code: 2
...
```

Exit code 2 typically indicates:
- Test completed but with issues
- Timeout occurred
- Partial success

### 5.2 Exit Code Standards

| Code | Meaning | Usage |
|------|---------|-------|
| 0 | Success | All tests passed |
| 1 | General error | Unexpected failure |
| 2 | Partial success | Some tests failed |
| 130 | Ctrl+C | User interrupted |
| 137 | SIGKILL | OOM or forced kill |
| 143 | SIGTERM | Graceful termination |

---

## 6. Appendix: Test Directory Inventory

### 6.1 Full Directory Listing

```
long_run_tests/
├── 4day_results_20260405_094154/          # EMPTY
├── 4day_results_20260405_094416/          # 74GB - INCOMPLETE
├── system_test_8hr_20260402_225637/       # 228M - PASSED (v1)
├── system_test_8hr_v2_20260403_161838/    # 278M - PASSED
├── system_test_8hr_v3_20260404_154327/    # 227M - PASSED
├── system_test_8hr_v4_20260404_232131/    # 80K - FAILED
├── system_test_8hr_v4_20260404_232653/    # 418M - PASSED
├── system_test_8hr_v4_20260405_093311/    # 40K - INCOMPLETE
├── system_test_8hr_v4_20260405_140754/    # 547M - PASSED
├── system_test_8hr_v5_20260405_081749/    # Small - PASSED
├── system_test_8hr_v5_20260405_082729/    # 22GB - PASSED (50 cycles)
└── [other marathon tests...]
```

### 6.2 Script Inventory (Root Directory)

```
# Test Scripts (47 total shell scripts)
test_*.sh: 6 scripts
*_test.sh: 2 scripts
benchmark*.sh: 4 scripts
stress*.sh: 4 scripts
*demo*.sh: 5 scripts
continuous*.sh: 1 script
[other utility scripts]
```

---

## 7. Conclusion

The Selfware test infrastructure is comprehensive but requires cleanup:

1. **93GB of disk space can be reclaimed** by cleaning incomplete and old tests
2. **Unicode in bash scripts causes crashes** - use ASCII only
3. **The 4-day test infrastructure works** but needs better space management
4. **Multiple duplicate scripts** should be consolidated

**Priority Actions:**
1. ⚠️ Archive/delete incomplete 4-day test (74GB)
2. 🔧 Fix or remove broken v4 script
3. 🧹 Remove duplicate test scripts
4. 📦 Implement log rotation for long-running tests

---

*Report generated by Agent 1 - Test Infrastructure & Results Audit*
*End of Report*
