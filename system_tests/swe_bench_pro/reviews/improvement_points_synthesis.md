# Synthesized Improvement Points — SWE-bench Pro Smaller Models

This document distills the findings from parallel agent analysis of the 10-model review. Items are grouped by failure mode and ranked by expected impact.

## Executive summary

The 0% overall pass rate across the 10 smaller models comes from three big buckets:

1. **The model never produces an applyable edit** → empty patches, apply failures, XML/tool-loop failures.
2. **The model produces a patch but on the wrong files / with wrong verification** → fail-to-pass tests still fail.
3. **The model produces a correct-looking patch but the harness mis-scores it** → test command / parser mismatches.

Fixing the high-impact items below should move the needle fastest.

---

## 1. Empty patches / agent-loop failures

### 1.1 Default small models to the agentless SEARCH/REPLACE path (HIGH)
- **Problem:** `llama-3.1-8b`, `qwen2.5-7b`, etc. are inferred as `small` tier but still run the multi-turn XML tool loop, which they cannot reliably drive; they exhaust `max_iterations` with empty patches.
- **Fix:** Make `--auto-agentless` the default for `recommended=false`/small-tier models; run `run_agentless` before the multi-turn agent loop.
- **Files:** `system_tests/swe_bench_pro/run_selfware.py` (`should_use_agentless`, `process_instance`)

### 1.2 Harvest SEARCH/REPLACE blocks from a failed agent run (MEDIUM)
- **Problem:** When the agent exits with `max_iterations` or JSON parse errors, any `### FILE:` / `<<<<<<< SEARCH` blocks it emitted in its final turn are discarded.
- **Fix:** After a failed agent run, scan the last response / `repo.selfware.stdout.log` for SEARCH/REPLACE blocks and attempt to apply them before falling back to diff generation.
- **Files:** `system_tests/swe_bench_pro/run_selfware.py`, `system_tests/swe_bench_pro/patch_utils.py`

### 1.3 Normalize `/app/...` paths before safety validation (MEDIUM)
- **Problem:** Agent stdout shows `Safety check failed: Path not in allowed list: /app`; the agent wastes iterations on absolute paths the prompt already forbids.
- **Fix:** Strip a leading `/app/` prefix and rewrite to a repo-relative path in the harness or in `src/agent/tool_dispatch.rs` before safety validation.
- **Files:** `system_tests/swe_bench_pro/run_selfware.py`, `src/agent/tool_dispatch.rs` or `src/safety/path_validator.rs`

---

## 2. Patch extraction / apply robustness

### 2.1 Make the SEARCH/REPLACE applier tolerant to small-model formatting (HIGH)
- **Problem:** `apply_edits_with_missing` requires exact `### FILE:` / `<<<<<<< SEARCH` / `>>>>>>> REPLACE` formatting and only strips one line-number gutter style; models emit numbered snippets, lowercase markers, extra whitespace, CRLF, etc.
- **Fix:** Make markers case- and whitespace-tolerant, strip additional gutter styles (`123 |`, `123:`, `123.`), and normalize blank-line differences before matching.
- **Files:** `system_tests/swe_bench_pro/patch_utils.py`

### 2.2 Broaden diff extraction and validate with `git apply --check` (MEDIUM)
- **Problem:** `extract_diff` only recognizes ` ```diff ` fences or bare `diff --git`; models also use ` ```patch ` and append prose after the diff.
- **Fix:** Accept ` ```patch ` / ` ```diff ` / bare `diff --git`, trim trailing prose after the last complete hunk, and validate with `git apply --check` before returning the patch.
- **Files:** `system_tests/swe_bench_pro/patch_utils.py`

### 2.3 Detect functional no-ops and reject trivial hunks (MEDIUM)
- **Problem:** The harness accepts patches that touch unrelated files or only add comments/imports/whitespace; pass-to-pass stays high but fail-to-pass still fails.
- **Fix:** After capturing the patch, intersect changed files with relevance-ranked sources and files implied by `fail_to_pass`/`test_patch`. Flag comment-only, import-only, or whitespace-only hunks as no-op and trigger recovery.
- **Files:** `system_tests/swe_bench_pro/run_selfware.py`, `system_tests/swe_bench_pro/patch_utils.py`, `system_tests/swe_bench_pro/small_model_adapter.py`

### 2.4 Improve diff filtering (MEDIUM)
- **Problem:** Substring filtering in `clean_captured_diff` / `filter_patch_to_source_files` can drop legitimate source files while keeping backup artifacts (`*.bak`, `*.orig`).
- **Fix:** Use anchored path/extension checks, explicitly reject backup copies, and log why hunks are dropped.
- **Files:** `system_tests/swe_bench_pro/patch_utils.py`, `system_tests/swe_bench_pro/run_selfware.py`

---

## 3. Test-command / verification mismatches

### 3.1 Fix `_format_test_command` for repo-specific runners (HIGH)
- **Problem:** The prompt tells the model generic commands (`npm test -- <files>` for NodeBB, plain `pytest` for qutebrowser, comma-joined Go args), but evaluation uses repo-specific invocations.
- **Fix:** Special-case `_format_test_command` per repo/language:
  - NodeBB: `npx mocha <test_files>` (not `npm test --`)
  - qutebrowser: `QT_QPA_PLATFORM=offscreen dbus-run-session -- python -m pytest ...`
  - Go: pass each selected test as a separate shell argument, not comma-joined
- **Files:** `system_tests/swe_bench_pro/small_model_adapter.py`, `system_tests/swe_bench_pro/run_selfware.py`

### 3.2 Pass selected test args as separate shell arguments in the entryscript (HIGH)
- **Problem:** `evaluate_predictions.py` comma-joins `selected_test_files_to_run` into one argument; Go subtest names containing commas are split incorrectly and reported as MISSING.
- **Fix:** Pass each selected argument as a separate `shlex.quote`-d shell argument to `run_script.sh` (`bash /workspace/run_script.sh "$arg1" "$arg2" ...`).
- **Files:** `system_tests/swe_bench_pro/evaluate_predictions.py` (`_build_entryscript`)

### 3.3 Harden scorer for collection/build/hook failures (MEDIUM)
- **Problem:** Collection/build/hook failures produce empty `tests` arrays or `NO_TESTS_FOUND_OR_PARSING_ERROR`, and FAIL_TO_PASS tests are marked MISSING instead of FAILED.
- **Fix:** Inspect `stdout.log`/`stderr.log` for collection/build/hook error patterns and convert MISSING FAIL_TO_PASS tests to FAILED with the detected reason.
- **Files:** `system_tests/swe_bench_pro/evaluate_predictions.py` (`evaluate_instance`, `_score_output`)

---

## 4. Prompt / ranking issues

### 4.1 Decode double-encoded instance metadata (HIGH)
- **Problem:** `problem_statement`, `requirements`, and `interface` are sometimes JSON-encoded strings embedded inside the JSONL value, so the prompt contains literal `\n` escapes and surrounding quotes, and the interface parser fails.
- **Fix:** Add a `_decode_json_string` helper and decode these fields once when loading an instance.
- **Files:** `system_tests/swe_bench_pro/run_selfware.py`, `system_tests/swe_bench_pro/small_model_adapter.py`

### 4.2 Fix test-file detection for NodeBB-style `test/` layouts (HIGH)
- **Problem:** `_extract_failing_test_snippets` skips files like `test/database/keys.js` because they neither end in `_test.js` nor start with `test_`; the focused test oracle is empty for NodeBB.
- **Fix:** Treat any file under `test/` or `tests/` as a test file and extract JS test names (`it('...')`, `describe('...')`, `def test_...`).
- **Files:** `system_tests/swe_bench_pro/small_model_adapter.py`

### 4.3 Extend interface parser for qutebrowser metadata (HIGH)
- **Problem:** `_parse_interface` only accepts `Path:`; qutebrowser instances use `Pathfile:`, so the Target API line for `widened_hostnames` in `qutebrowser/utils/urlutils.py` is dropped.
- **Fix:** Add `pathfile` (normalized to `path`) to the interface parser's known keys.
- **Files:** `system_tests/swe_bench_pro/small_model_adapter.py`

### 4.4 Add Python/JS identifier discovery to ranking functions (HIGH)
- **Problem:** `_find_function_definitions` and `_find_test_files` only match Go patterns, so Python identifiers like `widened_hostnames` and `_is_blocked` do not promote the real source/test files.
- **Fix:** Add regex branches for Python (`def ident(`, `class Ident`, async variants) and JS/TS in both functions.
- **Files:** `system_tests/swe_bench_pro/small_model_adapter.py`

### 4.5 Surface multi-file requirements from the interface (MEDIUM)
- **Problem:** NodeBB instance `0499…` requires `db.mget` in three database adapters plus `src/user/email.js`; models usually touch only one file.
- **Fix:** Parse `interface`/`requirements` for explicitly listed source files and pre-populate the editable manifest; add a planning instruction forcing the agent to list every file it must change.
- **Files:** `system_tests/swe_bench_pro/small_model_adapter.py`, `system_tests/swe_bench_pro/run_selfware.py`

### 4.6 Auto-wire newly created NodeBB route/controller files (MEDIUM)
- **Problem:** Instance `51d8f3b1…` requires creating and wiring `src/controllers/well-known.js` and `src/routes/well-known.js`; models create the files but omit the wiring.
- **Fix:** After patch capture, detect new files under `src/controllers/` or `src/routes/` and either auto-add a `require('./well-known')` export when unambiguous or inject a recovery prompt.
- **Files:** `system_tests/swe_bench_pro/run_selfware.py`, `system_tests/swe_bench_pro/patch_utils.py`

### 4.7 Forbid config-only edits in the prompt and patch filter (MEDIUM)
- **Problem:** Models sometimes edit `pytest.ini` / `setup.cfg` / `tox.ini`; these are filtered to empty, leaving no patch.
- **Fix:** Add these config files to the prompt’s “do not edit” rules and to non-source patterns.
- **Files:** `system_tests/swe_bench_pro/small_model_adapter.py`, `system_tests/swe_bench_pro/patch_utils.py`

---

## Suggested execution order

1. **Immediate (biggest lever):** 1.1, 3.1, 3.2, 4.2, 4.3, 4.4 — these fix the test-oracle and verification pipeline for the two dominant repos.
2. **Next:** 2.1, 2.2, 4.1 — improve patch capture and apply rates.
3. **Polish:** 2.3, 2.4, 1.2, 1.3, 3.3, 4.5, 4.6, 4.7 — reduce no-ops, wasted iterations, and scoring noise.
