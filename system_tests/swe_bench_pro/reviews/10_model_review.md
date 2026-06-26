# SWE-bench Pro 10-Model Review (Smaller Models)

This report aggregates the existing evaluation reports for 10 smaller-parameter models.

## Summary

- Models reviewed: **10**
- Average overall pass rate: **0.0%**
- Average fail-to-pass rate: **1.7%**
- Average pass-to-pass rate: **89.4%**

## Per-Model Results

| Model | Total | Completed | Errored | Overall Pass | Fail-to-Pass | Pass-to-Pass |
|-------|-------|-----------|---------|--------------|--------------|--------------|
| lfm-2.5-1.2b-thinking-free-sweap-v2 | 10 | 0 | 10 | 0/0 (0.0%) | 0.0% | 0.0% |
| llama-3.1-8b | 7 | 7 | 0 | 0/7 (0.0%) | 0.0% | 100.0% |
| qwen2.5-7b | 7 | 7 | 0 | 0/7 (0.0%) | 0.0% | 100.0% |
| mistral-nemo | 9 | 9 | 0 | 0/9 (0.0%) | 0.0% | 94.4% |
| nova-lite | 6 | 6 | 0 | 0/6 (0.0%) | 0.0% | 100.0% |
| granite-4.1-8b | 9 | 9 | 0 | 0/9 (0.0%) | 0.0% | 100.0% |
| gemma-3-12b | 9 | 9 | 0 | 0/9 (0.0%) | 0.0% | 100.0% |
| ling-2.6-flash | 8 | 8 | 0 | 0/8 (0.0%) | 0.0% | 100.0% |
| deepseek-v3.2 | 8 | 8 | 0 | 0/8 (0.0%) | 0.0% | 100.0% |
| xiaomi-mimo-v2.5 | 9 | 9 | 0 | 0/9 (0.0%) | 16.7% | 100.0% |

## Failure-Mode Breakdown

| Failure Mode | Instance Count |
|--------------|----------------|
| test failures | 72 |
| empty patch | 10 |

## Top 3 Failure Modes with Examples

### 1. NodeBB test failures (16 instances)

Most frequently failing tests in this repo:

| Failing Test | Count | Example Model / Instance |
|--------------|-------|--------------------------|
| `test/database.js | Test database test/database/keys.js::Key methods should return multiple keys and null if key doesn't exist` | 9 | llama-3.1-8b: instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan; qwen2.5-7b: instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan |
| `test/database.js | Test database test/database/keys.js::Key methods should return empty array if keys is empty array or falsy` | 9 | llama-3.1-8b: instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan; qwen2.5-7b: instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan |
| `test/user/emails.js | email confirmation (library methods) canSendValidation should return true if it has been long enough to re-send confirmation` | 9 | llama-3.1-8b: instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan; qwen2.5-7b: instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan |
| `test/controllers.js | Controllers .well-known webfinger should error if resource parameter is missing` | 7 | qwen2.5-7b: instance_NodeBB__NodeBB-51d8f3b195bddb13a13ddc0de110722774d9bb1b-vf2cf3cbd463b7ad942381f1c6d077626485a1e9e; mistral-nemo: instance_NodeBB__NodeBB-51d8f3b195bddb13a13ddc0de110722774d9bb1b-vf2cf3cbd463b7ad942381f1c6d077626485a1e9e |
| `test/controllers.js | Controllers .well-known webfinger should error if resource parameter is malformed` | 7 | qwen2.5-7b: instance_NodeBB__NodeBB-51d8f3b195bddb13a13ddc0de110722774d9bb1b-vf2cf3cbd463b7ad942381f1c6d077626485a1e9e; mistral-nemo: instance_NodeBB__NodeBB-51d8f3b195bddb13a13ddc0de110722774d9bb1b-vf2cf3cbd463b7ad942381f1c6d077626485a1e9e |

### 2. qutebrowser test failures (23 instances)

Most frequently failing tests in this repo:

| Failing Test | Count | Example Model / Instance |
|--------------|-------|--------------------------|
| `tests/unit/components/test_hostblock.py::test_subdomain_blocking` | 9 | llama-3.1-8b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171; qwen2.5-7b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171 |
| `tests/unit/utils/test_urlutils.py::TestWiden::test_widen_hostnames[a.b.c-expected0]` | 9 | llama-3.1-8b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171; qwen2.5-7b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171 |
| `tests/unit/utils/test_urlutils.py::TestWiden::test_widen_hostnames[foobarbaz-expected1]` | 9 | llama-3.1-8b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171; qwen2.5-7b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171 |
| `tests/unit/utils/test_urlutils.py::TestWiden::test_widen_hostnames[-expected2]` | 9 | llama-3.1-8b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171; qwen2.5-7b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171 |
| `tests/unit/utils/test_urlutils.py::TestWiden::test_widen_hostnames[.c-expected3]` | 9 | llama-3.1-8b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171; qwen2.5-7b: instance_qutebrowser__qutebrowser-c580ebf0801e5a3ecabc54f327498bb753c6d5f2-v2ef375ac784985212b1805e1d0431dc8f1b3c171 |

### 3. empty patch (10 instances)

Example occurrences:

- xiaomi-mimo-v2.5: `instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan`
- xiaomi-mimo-v2.5: `instance_qutebrowser__qutebrowser-f91ace96223cac8161c16dd061907e138fe85111-v059c6fdc75567943479b23ebca7c07b5e9a7f34c`
- xiaomi-mimo-v2.5: `instance_NodeBB__NodeBB-51d8f3b195bddb13a13ddc0de110722774d9bb1b-vf2cf3cbd463b7ad942381f1c6d077626485a1e9e`

## Recommended Next Fixes

1. **Patch quality**: the high share of empty / apply-failed / no-op patches suggests the edit extraction and agent loop still need tightening; see `patch_utils.py` and `run_selfware.py` recovery paths.
2. **Fail-to-pass focus**: when tests do run, fail-to-pass tests almost never pass. The focused test oracle and post-edit test command should help, but the model may need stronger hints about the exact assertion to satisfy.
3. **Per-language test parsing**: the most frequent failing tests cluster in NodeBB and qutebrowser; verify that the test command formatter and parser correctly map the official test names in `small_model_adapter.py` / `evaluate_predictions.py`.
