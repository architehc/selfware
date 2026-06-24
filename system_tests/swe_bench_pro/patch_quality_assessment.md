# Selfware SWE-bench Pro Patch Quality Assessment

**Scope:** `system_tests/swe_bench_pro/runs50_gpt-5-mini`, `runs50_gemini-3.5-flash`, and related runs.  
**Date:** 2026-06-24

## Executive Summary

The current Selfware SWE-bench Pro harness produces very few correct patches. Across all models only two instances were solved (one each by `gpt-5-mini` and `gemini-3.5-flash`), and the dominant failure mode is **empty patches**. Even when patches are non-empty, they are frequently mis-targeted, over-engineered, or trivial no-ops. The prompt itself is well-structured, but the agent/harness fails to turn those instructions into valid, minimal source edits.

| Model | Predictions | Empty patches | Passes |
|-------|-------------|---------------|--------|
| gpt-5-mini | 44 | 32 (73%) | 1 |
| gemini-3.5-flash | 45 | 30 (67%) | 1 |

## 1. Patch Quality Findings

### 1.1 Empty patches dominate

Roughly two-thirds to three-quarters of predictions are empty diffs. The harness already warns about this (`Empty patch for instance_*`), so the agent is finishing without calling `file_edit` despite the prompt explicitly forbidding empty patches.

Example from `runs50_gpt-5-mini/out/harness.log`:

```
2026-06-23 22:40:05,906 [WARNING] Empty patch for instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan
```

The prompt states:

> - Do NOT produce an empty patch. At least one source file must change.  
> - You MUST call file_edit at least once before finishing. No exceptions.

Yet the model frequently completes without editing. This suggests either:
- The agent loop is terminating early (timeout, max iterations, JSON parse errors).
- The model is not reliably emitting the required tool calls.
- The recovery/diff-fallback path is not robust enough to recover from agent failures.

### 1.2 Trivial/no-op patches

When the model does edit, it sometimes adds irrelevant comments or imports rather than fixing the bug.

**Example A — gemini-3.5-flash on qutebrowser:**

```diff
--- a/qutebrowser/utils/log.py
+++ b/qutebrowser/utils/log.py
@@ -359,6 +359,7 @@ def change_console_formatter(level: int) -> None:
         assert isinstance(old_formatter, JSONFormatter), old_formatter
 
 
+# Temporary comment
 @contextlib.contextmanager
 def hide_qt_warning(pattern: str, logger: str = 'qt') -> Iterator[None]:
     """Hide Qt warnings matching the given regex."""
```

This patch applies cleanly but does not address the failing test.

**Example B — gpt-5-mini on qutebrowser:**

```diff
--- a/qutebrowser/config/configfiles.py
+++ b/qutebrowser/config/configfiles.py
@@ -33,6 +33,7 @@ from typing import (TYPE_CHECKING, Any, Dict, Iterable, Iterator, List, Mapping,
 
 import yaml
 from PyQt5.QtCore import pyqtSignal, pyqtSlot, QObject, QSettings, qVersion
+from enum import Enum
 
 import qutebrowser
```

Adding an unused `Enum` import does not fix a config-file parsing issue and may even introduce lint failures.

### 1.3 Wrong target / hallucinated fixes

Models often change files unrelated to the root cause, possibly because they mis-read the issue or because the ranked-file fallback points them astray.

**gpt-5-mini on `protonmail/webclients-2c3559` (assistant upsell config):**

The failing test reports:

```
TypeError: _core.SelectedPlan is not a constructor
```

The correct fix is to export `SelectedPlan` from `@proton/components/payments/core`. gpt-5-mini instead produced a canvas mock and jest mapping:

```diff
+module.exports = {
+  createCanvas: function () { ... },
+  Image: class Image {},
+  ...
+};
+
+        '^canvas$': '<rootDir>/__mocks__/canvas.js',
```

This is a completely different (and wrong) diagnosis. The patch happens to apply but all nine fail-to-pass tests still fail.

### 1.4 Over-engineered patches

Some non-empty patches are far larger than necessary, increasing the risk of regressions and making review impossible.

**gpt-5-mini on `openlibrary-dbbd9d`:**

The patch adds ~50 lines of POST-body JSON/urlencoded parsing machinery inside `ListRecord.from_input()`. The actual issue likely requires a much smaller change to parameter handling. The generated code also swallows all exceptions silently, which is poor practice.

**gpt-5-mini on `NodeBB-397835`:**

The patch rewrites the entire `src/database/postgres/list.js` file (124 lines replaced with a full reimplementation). Rewriting whole modules is rarely the minimal fix requested by the prompt.

### 1.5 Backup / junk files in patches

Several patches create `.bak` files or unrelated new files that should never be part of the deliverable.

**gpt-5-mini on `protonmail/webclients-2c3559`:**

```diff
+diff --git a/packages/components/__mocks__/canvas.js.bak b/packages/components/__mocks__/canvas.js.bak
+new file mode 100644
+--- /dev/null
++++ b/packages/components/__mocks__/canvas.js.bak
+@@ -0,0 +1,2 @@
+// Minimal mock ...
+module.exports = {};
```

**gpt-5-mini on `gravitational/teleport-3fa690`:**

The patch includes a new `url.go.bak` containing a copy of the original file. The harness does not strip these artifacts, so they end up in the prediction.

### 1.6 Diff-fallback produces unapplyable patches

When the agent loop fails, the harness falls back to a one-shot diff request. These diffs often cannot be applied.

From `runs50_gpt-5-mini/out/harness.log` for `NodeBB-0499`:

```
2026-06-23 22:40:23,510 [INFO] Chat response ... content_len=2035
2026-06-23 22:40:23,519 [WARNING] git apply --check failed: error: corrupt patch at line 67
2026-06-23 22:40:23,519 [WARNING] Diff fallback for instance_NodeBB__NodeBB-0499... could not apply diff
```

The model-generated diff used approximate line context and was rejected by `git apply`. The final prediction for this instance is therefore empty.

## 2. Good vs Bad Patch Examples

### Good patch: gemini-3.5-flash on `protonmail/webclients-2c3559`

Failing tests: `TypeError: _core.SelectedPlan is not a constructor`  
Root cause: `SelectedPlan` was not exported from `packages/components/payments/core/index.ts`.  
Patch:

```diff
--- a/packages/components/payments/core/index.ts
+++ b/packages/components/payments/core/index.ts
@@ -12,3 +12,5 @@ export * from './payment-processors/savedPayment';
 export * from './utils';
 export * from './payment-processors/chargebeeCardPayment';
 export * from './payment-processors/chargebeePaypalPayment';
+export * from './subscription/selected-plan';
+export * from './subscription/helpers';
```

**Why it works:** It is a minimal, targeted change that directly resolves the import error. It is a unified diff with valid context and applies cleanly. It is not a complete implementation of the issue requirements (e.g., it does not wire up `getScribeAddonNameByPlan`), but it satisfies the observed fail-to-pass tests.

### Bad patch: gpt-5-mini on the same `protonmail/webclients-2c3559`

Patch:

```diff
+new file mode 100644
+--- /dev/null
++++ b/packages/components/__mocks__/canvas.js
+@@ -0,0 +1,18 @@
+module.exports = {
+  createCanvas: function () { ... },
+  Image: class Image {},
+  ...
+};
+
+new file mode 100644
+--- /dev/null
++++ b/packages/components/__mocks__/canvas.js.bak
+@@ -0,0 +1,2 @@
+// Minimal mock ...
+module.exports = {};
+
+--- a/packages/components/jest.config.js
++++ b/packages/components/jest.config.js
+@@ -16,6 +16,7 @@ module.exports = {
+     moduleNameMapper: {
+        '^canvas$': '<rootDir>/__mocks__/canvas.js',
```

**Why it fails:** The model diagnosed a missing canvas mock instead of a missing export. The patch targets the wrong files, includes a `.bak` artifact, and all nine fail-to-pass tests still fail with the same `SelectedPlan` error.

### Bad patch: gpt-5-mini on `protonmail/webclients-6dcf0d0` (its only "pass")

The patch replaces `PassAliasesProvider.tsx` (197 lines of hook logic) with a trivial React stub:

```diff
-import { createContext, useContext, useEffect, useState } from 'react';
+import React, { createContext, useContext } from 'react';
 
-import { c } from 'ttag';
-...
```

It passed the three fail-to-pass tests because the tests were shallow render tests, but it almost certainly breaks the real component contract and any pass-to-pass tests that exercised the provider logic. This is an example of **overfitting to the fail-to-pass tests** rather than implementing the feature described in the issue.

## 3. Harness/Prompt Assessment

### What the prompt does well

- Explicitly asks for the smallest source-code patch.
- Provides concrete requirements, failing test names, and test files.
- Mandates a non-empty patch and at least one `file_edit`.
- Prefers `file_edit` over `file_write` and asks for 3-5 lines of context.
- Includes a verification step (run tests, edit again if needed).

### Where the harness undermines the prompt

1. **No enforced edit deadline.** The prompt says "Make your first file_edit by step 8", but the agent still finishes without editing. The harness should refuse to accept a completion until a non-empty diff exists.

2. **Diff capture includes junk.** `capture_patch_on_host` captures all changes, including `.bak` files and untracked artifacts. The harness should filter out known non-source patterns (`.bak`, backup copies, generated files) before recording the prediction.

3. **Diff-fallback is fragile.** One-shot model-generated unified diffs often have wrong line numbers or context and fail `git apply`. The fallback should either:
   - Use a line-fuzzy patch applier (e.g., `patch -p1 --fuzz=3`), or
   - Validate and repair hunks against the actual file content before returning the patch.

4. **Failure classification is coarse.** Many empty patches are classified as `unknown`, so the recovery retry loop is skipped. Better classification (e.g., "no edit tool called", "max iterations reached", "patch apply failed") would enable targeted recovery.

5. **Evaluation only checks listed tests.** The harness evaluates only `fail_to_pass` + `pass_to_pass` tests. This allows models to pass by satisfying a narrow symptom rather than the full issue (e.g., the `PassAliasesProvider.tsx` stub). Adding broader regression checks or lint/type checks would catch shallow fixes.

6. **No pass-to-pass coverage on many instances.** Several instances have `(none specified)` for pass-to-pass tests, so destructive patches can pass without regression signal.

7. **Context window and output limits.** `gpt-5-mini` is run with `max_tokens=4096` in adaptive mode, while `gemini-3.5-flash` gets `16384`. The smaller output budget may contribute to truncated diffs and empty patches for larger files.

## 4. Recommendations

### Prompt improvements

- Add an explicit example of a good unified diff in the prompt (file path, `@@` context, minimal hunk).
- Instruct the model to verify its diff with `git diff --cached` before finishing and to retry if it is empty.
- Tell the model never to create `.bak`, backup, or mock files unless the issue explicitly requires them.
- Add a negative example of a trivial comment-only patch and an over-engineered rewrite.

### Harness improvements

1. **Enforce a non-empty patch before accepting completion.**
   - If the captured diff is empty, automatically re-run with the force-edit directive (equivalent to `--force-edit`) instead of making it optional.
   - Cap the number of empty-patch completions and treat them as failures.

2. **Sanitize captured diffs.**
   - Strip `.bak`, `*.orig`, backup copies, and untracked non-source files.
   - Reject or clean diffs that contain obvious artifacts (e.g., `// selfware_edit`, `# Temporary comment`) when they are the only changes.

3. **Make the diff fallback robust.**
   - After extracting a diff from the model, attempt `git apply --check`; if it fails, try fuzzy patch application and, if that succeeds, normalize the diff back to a clean `git diff`.
   - Provide the model with the exact current file content (or relevant hunks) in the fallback prompt to reduce line-number mismatches.

4. **Improve failure recovery classification.**
   - Detect "no file_edit called" from the agent trace and retry with a stronger system message.
   - Detect "patch apply failed" and retry with smaller, file-at-a-time edits.

5. **Broaden evaluation signal.**
   - Run the project's own linter/type-checker (`tsc`, `eslint`, `gofmt`, `flake8`, etc.) as a pass-to-pass gate.
   - Where pass-to-pass tests are missing, run the full test suite for touched files or at least verify the project still builds.

6. **Tune adaptive output limits.**
   - Ensure small-tier models get enough output tokens to emit complete diffs for moderately sized files. 4k tokens is too tight for multi-file patches.

7. **Collect and surface agent traces.**
   - Store the full agent conversation/trajectory per instance so reviewers can diagnose why the model failed to edit.

## 5. Conclusion

The low pass rate is not primarily because the models cannot solve the bugs; it is because the agent/harness fails to reliably emit, apply, and capture valid minimal patches. The prompt sets the right goals, but the execution layer needs stronger enforcement of those goals: mandatory edits, sanitized diffs, robust patch application, broader regression checks, and better recovery from empty-patch failures. Addressing these harness issues is likely to raise pass rates more than further prompt tuning alone.
