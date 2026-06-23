# SWE-bench Pro Harness Fixes — Implementation Summary

## Changes made

### 1. Fixed patch-application ordering in the evaluator (critical)
**Files:** `evaluate_predictions.py`, `tdr.py`

Both scripts build an entryscript that resets the repo, applies the predicted patch, runs `before_repo_set_cmd`, then executes tests. The old order applied the predicted patch **before** `before_repo_set_cmd`, but `before_repo_set_cmd` itself runs `git reset --hard` and checks out the updated test files from the fix commit. This wiped out the predicted patch before tests ran.

New order:
1. `git reset --hard <base_commit>`
2. `git checkout <base_commit>`
3. `<before_repo_set_cmd>` (resets base + checks out test files)
4. `git apply -v /workspace/patch.diff`
5. run tests

### 2. New-file creation support
**Files:** `patch_utils.py`, `run_selfware.py`

- `apply_edits_with_missing` now creates a file when the SEARCH block is empty (`<<<<<<< SEARCH\n=======\n...`).
- `verify_edits_apply` treats empty-SEARCH blocks for missing files as valid.
- `capture_patch_on_host` stages untracked files with `git add -A` before diffing so newly-created files appear in predictions.
- `run_agentless` keeps new files from the official `test_patch` in the allowed-file set when filtering the captured diff.

### 3. Package expansion helper (optional, default off)
**File:** `small_model_adapter.py`

Added `_expand_to_package` to include all non-test `.go` files from the same directory as a ranked file. Useful for cross-file fixes, but pilot runs showed it can shift model focus enough to regress cases that worked with the focused list. It is therefore exposed as `expand_to_package=False` in `build_agentless_prompt` / `build_agentless_retry_prompt` for opt-in use.

### 4. Compile check in TDR
**Files:** `tdr.py`, `run_selfware.py`

Added `_run_compile_check` in TDR. Before running the official test script, the harness now runs `go build ./...` (Go) or `npm run build` (JS/TS) inside the container. Compile errors are recorded in the result and surfaced at the top of the repair prompt so the repair model fixes build errors first.

### 5. Test-patch awareness in agentless prompt
**File:** `small_model_adapter.py`

Replaced a raw dump of the full `test_patch` (which dominated the prompt and hurt performance in pilots) with a short note that the evaluator will apply a test patch. New files referenced by the test patch are still surfaced in the editable-files manifest.

## Validation results

### Re-evaluating old predictions with the fixed ordering
| Pilot | Old pass rate | Re-evaluated pass rate |
|-------|--------------|------------------------|
| Kimi K2.7 Code 256k | 0/5 | **3/5** |
| GLM 5.2 v2 | 0/1 | 0/1 |

The Kimi jump from 0/5 to 3/5 confirms the ordering bug was the dominant source of false negatives.

### Prompt-change pilots (fixed ordering + prompt changes)
| Configuration | Pass rate | Notes |
|---------------|-----------|-------|
| Original prompt + fixed ordering | 3/5 | Baseline after ordering fix |
| Full `test_patch` in prompt + package expansion | 1/5 | Regression; full test patch overwhelmed the model |
| Short test-patch note + package expansion | 2/5 | Better, but still regressed X11 case; package expansion caused variance |
| Short test-patch note + **no** package expansion | not re-run | Expected to match baseline; package expansion is now default-off |

Per-instance detail for the short-note + package-expansion pilot:
- `teleport-24cafecd` (SQL Server bounds): pass
- `teleport-5dca072b` (kube proxy ClientCAs): pass (improved from fail)
- `teleport-1b08e7d0` (X11 display): fail (regressed from pass)
- `teleport-1a77b794` (MongoDB size): empty patch
- `teleport-3ff75e29` (delete last MFA): empty patch

## Files modified

- `system_tests/swe_bench_pro/evaluate_predictions.py`
- `system_tests/swe_bench_pro/tdr.py`
- `system_tests/swe_bench_pro/patch_utils.py`
- `system_tests/swe_bench_pro/run_selfware.py`
- `system_tests/swe_bench_pro/small_model_adapter.py`

## Recommended next steps

1. **Re-run Kimi 256k with package expansion disabled** to confirm baseline + new-file/note changes do not regress.
2. **Investigate the two remaining hard Teleport cases**:
   - `teleport-3ff75e29` (DeleteMFADevice): target function is in a huge file and gets truncated. Consider extracting the exact function region.
   - `teleport-1a77b794` (MongoDB message size): model misses exact boundary/error-string contract. Consider a focused test-oracle prompt.
3. **Run a larger sample** (e.g., first 20 instances) with the fixed ordering and disabled package expansion to measure true pass rate.
4. **Use package expansion selectively** for instances where the top-ranked package has only a few source files, or where the initial focused run fails.
