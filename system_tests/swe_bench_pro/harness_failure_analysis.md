# SWE-bench Pro Harness Failure Analysis

## Executive Summary

Across all 27+ completed model runs, overall pass rates are near 0% (0–6.5%). The dominant failure mode is **not** that patches are applied and then break tests. Instead, the evaluation harness usually runs tests on the **unpatched base commit**, yielding ~100% pass-to-pass and ~0% fail-to-pass. This is caused by a fragile patch-application step in the evaluation entryscript that silently continues when `git apply` fails or when the submitted patch is empty.

## Top Failure Patterns

### 1. Evaluation entryscript silently ignores patch-apply failures

**File:** `system_tests/swe_bench_pro/evaluate_predictions.py` (lines 132–146)

The generated container entryscript is:

```bash
#!/bin/bash
cd /app
git reset --hard {base_commit}
git checkout {base_commit}
{before_cmd}
git apply -v /workspace/patch.diff
bash /workspace/run_script.sh {test_arg} > /workspace/stdout.log 2> /workspace/stderr.log
python /workspace/parser.py ...
```

Problems:
- No `set -e` / `set -o pipefail`. If `git apply` fails, the script continues and runs tests on the unpatched repository.
- Only a single `git apply -v` attempt; no fallback to `git apply --3way` or `patch -p1`.
- The `git apply -v` output is **not** captured to `stdout.log`/`stderr.log`, so apply failures are invisible in the logs.

**Evidence:**
- Universal pattern across runs: pass-to-pass is typically 94–100%, while fail-to-pass is 0–3%.
  - `runs50_gemini-3.5-flash`: pass-to-pass 5754/5754 (100%), fail-to-pass 9/284 (3.17%).
  - `runs10_qwen3-32b`: pass-to-pass 723/884 (81.8%), fail-to-pass 0/72 (0%).
  - `runs10_mistral-large-3`: pass-to-pass 870/922 (94.4%), fail-to-pass 0/108 (0%).
- This is exactly the signature of tests running on the base commit: pre-existing tests still pass, bug-revealing tests still fail.

### 2. Empty patches are treated as success

Many runs produce empty predictions (`"patch": ""`). The harness writes an empty `/workspace/patch.diff`, `git apply` succeeds as a no-op, and the evaluator reports the instance as "completed" with 100% pass-to-pass and 0% fail-to-pass.

**Concrete counts:**

| Run | Predictions | Empty patches | Empty % |
|-----|-------------|---------------|---------|
| `runs10_mistral-large-3` | 9 | 9 | 100% |
| `runs10_ling-2.6-flash` | 8 | 8 | 100% |
| `runs10_gemma-3-12b` | 9 | 9 | 100% |
| `runs50_kimi-k2.6` | 21 | 21 | 100% |
| `runs50_gpt-5-mini` | 44 | 32 | 73% |
| `runs50_glm-5.2-nitro` | 46 | 37 | 80% |
| `runs10_qwen3-32b` | 7 | 1 | 14% |

**Evidence:** `system_tests/swe_bench_pro/runs10_mistral-large-3/out/predictions.jsonl` contains only `"patch": ""` entries.

### 3. Generation harness is more robust than evaluation harness

**Files:** `system_tests/swe_bench_pro/patch_utils.py` vs. `evaluate_predictions.py`

The generation-side `apply_patch()` helper:
1. Tries `git apply`
2. Falls back to `git apply --3way`
3. Falls back to `patch -p1`
4. Returns `True`/`False` and logs stderr

The evaluation-side entryscript does none of this. Patches that the generation phase can apply (with fallback) may still fail during evaluation, and the failure is hidden.

### 4. Predicted patches are often syntactically invalid or wrong-targeted

Even when patches are non-empty, they frequently do not modify the actual source files that need fixing.

**Examples:**

- `runs10_qwen3-32b` / `instance_ansible__ansible-f327e65d...`  
  Predicted patch (`eval/instance_ansible__ansible-f327e65d...patch.diff`) replaces the entire `lib/ansible/galaxy/dependency_resolution/dataclasses.py` (451 lines) with a single line:
  ```python
          all(part and not iskeyword(part) and part.isidentifier() for part in ns_or_name.split('.'))
  ```
  This corrupts the module; the 4 fail-to-pass tests all fail with assertion errors, while 62/171 pass-to-pass tests still pass.

- `runs10_qwen3-32b` / `instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan`  
  Predicted patch creates a **new** file `database/redis/main.js` instead of modifying the existing source files (`public/language/en-GB/admin/manage/users.json`, `public/language/en-GB/error.json`, etc.) that the ground-truth patch touches. The new file is never imported, so the bug remains unfixed.

- `runs10_qwen3-32b` / `instance_qutebrowser__qutebrowser-f91ace96223cac8161c16dd061907e138fe85111-v059c6fdc75567943479b23ebca7c07b5e9a7f34c`  
  Predicted patch creates a new test file `qutebrowser/tests/test_utils.py` and truncates/replaces `qutebrowser/utils/qtlog.py` with an incomplete snippet, rather than editing `qutebrowser/browser/qtnetworkdownloads.py` and `qutebrowser/utils/log.py` as the ground-truth patch does.

### 5. Instances where the patch looks partially correct but still fails

`instance_qutebrowser__qutebrowser-f631cd4422744160d9dcf7a0455da532ce973315-v35616345bb8052ea303186706cec663146f0f184` is the only instance that regularly shows non-zero fail-to-pass scores (5–30/32 across runs). The predicted patch modifies the same documentation files as the ground-truth patch (`doc/changelog.asciidoc`, `doc/help/settings.asciidoc`), and the pass-to-pass list is empty (`pass_to_pass: []`). Because the fix is incomplete or the checked-out test file (`tests/unit/config/test_configfiles.py`) expects source changes in `qutebrowser/config/configfiles.py`, the instance never fully passes, but it demonstrates that the harness **can** apply patches and run tests when the model produces a plausible diff.

## Recommended Harness Improvements

1. **Fail fast on patch-apply failure**
   - Add `set -euo pipefail` to the evaluation entryscript.
   - Check `git apply` exit code explicitly and record it as an instance error if non-zero.

2. **Use the same robust patch application in evaluation as in generation**
   - Reuse `patch_utils.apply_patch()` (or equivalent) in the container: try `git apply`, then `git apply --3way`, then `patch -p1`.

3. **Capture and persist patch-apply output**
   - Redirect `git apply -v` output to a log file (e.g., `/workspace/patch_apply.log`) and copy it out of the container.
   - Include patch-apply exit status in `evaluation_report.json`.

4. **Reject empty/no-op patches**
   - If `prediction["patch"]` is empty or whitespace-only, mark the instance as errored or at least log it; do not run a full test suite on the unpatched repo.

5. **Detect unpatched evaluation signatures**
   - Add a sanity check: if fail-to-pass is 0 and pass-to-pass is 100% (or tests match the base-commit baseline), flag the instance for manual review as a likely patch-apply failure.

6. **Fix container-logging duplication**
   - `evaluate.log` shows every "Starting container" / "Stopping and removing container" line duplicated, which makes debugging harder. Ensure the logger is not attached twice.

## Files Referenced

- `system_tests/swe_bench_pro/evaluate_predictions.py` — evaluation entryscript and scoring
- `system_tests/swe_bench_pro/patch_utils.py` — generation-side robust patch application
- `system_tests/swe_bench_pro/runs*/eval/evaluation_summary.md` — per-run summaries
- `system_tests/swe_bench_pro/runs*/eval/*.patch.diff` — actual diffs submitted to the harness
- `system_tests/swe_bench_pro/runs*/out/predictions.jsonl` — model predictions
- `system_tests/swe_bench_pro/sample_50.jsonl` — ground-truth patches and test metadata
