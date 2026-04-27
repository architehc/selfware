# SWE-bench Pro × selfware × Qwen3.6 quants — 2026-04-27

Three SWE-bench Pro instances (qutebrowser/python, NodeBB/js,
flipt-io/go), every available Qwen3.6-27B HauhauCS quant + the
Qwen3.6-35B-A3B-Q3_K_XL baseline. 33 (instance × quant) pairs,
~140 min agent runtime on 2× RTX 4090, llama-server with
256k ctx + q8_0 KV cache + `--parallel 2` continuous batching.

This page describes **two** runs of the same harness on the same
data:

1. **Pre-fix run (initial)** — selfware's ESCALATED progress guard
   force-wrote a generic Rust scaffold to `src/lib.rs` whenever the
   agent stalled, regardless of whether the workdir was a real
   codebase. 21 of 33 patches were byte-identical scaffolds that
   masked the true model behaviour.
2. **Post-fix run (after `642e7ed3`)** — the scaffold injection now
   refuses to fire when the workdir already contains source files.
   This unblocks the bench but reveals that the **agent loop itself
   gives up** on these tasks before producing edits.

## Headline (post-fix)

**1/33 instances received a real (non-empty, non-scaffold) edit attempt.**
Zero passed `cargo test` / language-equivalent.

| Quant | qutebrowser | NodeBB | flipt-io | bytes |
|---|---|---|---|---:|
| Qwen3.6-35B-A3B-Q3_K_XL | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-Q8_K_P | empty | empty | **342B (real)** | 342 |
| Qwen3.6-27B-HauhauCS-Q6_K_P | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-Q5_K_P | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-Q4_K_P | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-Q3_K_P | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-Q2_K_P | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-IQ4_XS | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-IQ3_M | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-IQ3_XS | empty | empty | empty | 0 |
| Qwen3.6-27B-HauhauCS-IQ2_M | empty | empty | empty | 0 |

### The one real attempt

Q8_K_P on `flipt-io/flipt` produced exactly the right starting move:

```diff
diff --git a/rpc/flipt/flipt.proto b/rpc/flipt/flipt.proto
@@ -68,6 +68,7 @@ message EvaluationResponse {
   string value = 8;
   double request_duration_millis = 9;
   string attachment = 10;
+  string reason = 11;
 }
```

The issue asked literally for `Add a "reason" field to the
EvaluationResponse payload`. The model edited the right file, used
the next available proto field number, and stopped. The full gold
patch goes further (regenerated Go code, evaluation logic, tests)
so this is necessary-but-not-sufficient — the eval would still
score it as failed — but it's the only run on the entire sweep
where the model touched the actual problem.

There's also one **single-trial earlier verification run** outside
the sweep where Q4_K_P produced a 3178-byte patch on qutebrowser:
right file (`qutebrowser/misc/guiprocess.py`), right semantic idea
(`if self.outcome.was_successful(): all_processes.pop(self.pid, None)`),
but with the same edit duplicated 7× in a degenerate loop. That run
isn't in the sweep table because the sweep's Q4_K_P × qutebrowser
trial separately produced 0B. **Single-trial variance is large** on
these long-horizon tasks.

## Why "empty" dominates

A representative agent log from the sweep
(Q4_K_P × qutebrowser, post-fix):

```
Step 9 ✗ PROGRESS GUARD: 7 consecutive read-only steps … blocked
Step 11 ✗ PROGRESS GUARD: 8 consecutive read-only steps
Step 12 ✗ RETRY SUPPRESSED: grep_search blocked
Step 13–15 (more retry suppression)
[exit, 0-byte patch]
```

The model:
1. Reads the failing test file.
2. Greps for related symbols.
3. Reads more code.
4. Selfware's "read_loop" guard fires at ~step 8 and blocks further
   read-only tools.
5. Instead of pivoting to `file_edit`, the model retries the same
   read tool, gets blocked again, and eventually exits.

This is a real product issue separate from the scaffold bug — the
model isn't internalising the directive to switch from reading to
editing. Possible mitigations:
- More forceful nudges in the system directive when the guard fires
  ("you MUST call `file_edit` next; no other tool will be permitted").
- Inject a synthetic example tool-call sequence in the directive so
  the model has a template to copy.
- Lower the read-only-step threshold for the warning, leave the hard
  block at 12+ instead of 8.

## What the harness now produces correctly

After `642e7ed3`:

- Real patches when the model engages (Q8_K_P × flipt example above).
- Empty patches when the model gives up (no false-positive scaffold
  pollution).
- Per-pair `agent.log` preserved so any "the model tried but
  selfware blocked it" pattern is inspectable.

## What's still missing

1. **Multi-trial averaging.** 33 single trials with this much
   variance gives noisy data. 3-5× retries per (instance × quant)
   would let us report median bytes / median pass and have actual
   confidence intervals.
2. **Docker eval.** We have not run the official
   `swe_bench_pro_eval.py` — every patch is either empty or
   incomplete, so the eval would return 0% across the board for ~50
   GB of dockerhub pulls.
3. **Read-loop UX fix.** Until the model reliably pivots to writing
   when blocked from reading, even strong quants will look weak on
   long-horizon tasks.

## Reproducing

```bash
# Pull dataset + run all 11 quants × first 3 (shortest-prompt) instances
python3 scripts/swebench_pro/run.py --quants all --instances 3

# Or specify your own instance ids:
python3 scripts/swebench_pro/run.py \
    --quants Qwen3.6-27B-HauhauCS-Q4_K_P \
    --instance-ids instance_qutebrowser__qutebrowser-c09e1439f145c66ee3af574386e277dd2388d094-v2ef375ac784985212b1805e1d0431dc8f1b3c171
```

Outputs land in `reports/swebench_pro/<timestamp>/runs/<quant>/<instance_id>/`:
- `repo/` — cloned + checked out workdir
- `prompt.txt` — exactly what the agent saw
- `instance.json` — full HF dataset row
- `agent.log` — full selfware stdout (very useful for diagnosing read_loop)
- `<instance_id>.pred` — captured diff
- `result.json` — exit code + wall time + patch size
