# SWE-bench Pro × selfware × Qwen3.6 quants — 2026-04-27

Three SWE-bench Pro instances (qutebrowser/python, NodeBB/js,
flipt-io/go), every available Qwen3.6-27B HauhauCS quant + the
Qwen3.6-35B-A3B-Q3_K_XL baseline. 33 (instance × quant) pairs,
~100 min total agent runtime on 2× RTX 4090, llama-server with
256k ctx + q8_0 KV cache + `--parallel 2` continuous batching.

## Headline

**0/33 instances genuinely fixed.** Every quant scored 0/3 on this
subset.

| Quant | qutebrowser | NodeBB | flipt-io | Real |
|---|---|---|---|---:|
| Qwen3.6-35B-A3B-Q3_K_XL | empty | empty | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-Q8_K_P | empty | empty | empty | 0/3 |
| Qwen3.6-27B-HauhauCS-Q6_K_P | empty | empty | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-Q5_K_P | empty | empty | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-Q4_K_P | scaffold | empty | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-Q3_K_P | scaffold | scaffold | empty | 0/3 |
| Qwen3.6-27B-HauhauCS-Q2_K_P | empty | scaffold | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-IQ4_XS | scaffold | empty | empty | 0/3 |
| Qwen3.6-27B-HauhauCS-IQ3_M | scaffold | empty | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-IQ3_XS | scaffold | scaffold | scaffold | 0/3 |
| Qwen3.6-27B-HauhauCS-IQ2_M | scaffold | empty | scaffold | 0/3 |

`empty` = 0-byte patch (agent gave up without writing anything)
`scaffold` = a 357-byte patch that's **byte-identical (md5 `62c7ecd1`)** across every "scaffold" cell — see below.

## What the "scaffold" patch actually contains

Every non-empty pred file is the same 5-line stub:

```diff
diff --git a/src/lib.rs b/src/lib.rs
new file mode 100644
+// AUTO-SCAFFOLD: fill in the implementation
+// Task: You are working on a real codebase in the current directory. Resolve this issue:
+
+// TODO: implement the functions described in the task
+// Then run cargo test to verify
```

It's emitted whether the underlying repo is Rust, Go, or JS. It
overwrites or creates `src/lib.rs` regardless of the project's actual
layout.

## Where the scaffold comes from — and why this is a selfware bug

`src/agent/execution.rs:515` has an **ESCALATED progress guard**:
when the agent accumulates more than ~20 consecutive read-only steps,
selfware injects a synthetic `file_write` tool call that creates
`src/lib.rs` with a hardcoded boilerplate, then sets
`has_written_any_file = true` and rewrites the system directive to
tell the model "now implement the full solution."

The logic was designed for the SAB harness's small Rust scratch
projects, where `src/lib.rs` is the right place to scaffold against a
known-empty workspace. In any real-world codebase — including every
SWE-bench Pro instance — it's harmful:

- Wrong path: most repos have `src/lib.rs` at a different location
  (or no Rust at all).
- Wrong format: it writes Rust to Go, JS, Python repos.
- Wrong scope: it dumps the user task as a comment instead of
  reading existing code and editing it.

This is what's blocking real evaluation against SWE-bench Pro for
every quant we tested. The model itself never gets a chance to
demonstrate capability on these tasks because selfware's safety
fallback fires first.

## What we still learned

1. **The harness works end-to-end.** `scripts/swebench_pro/run.py`
   correctly loads the dataset, clones each repo at the right
   `base_commit`, builds a prompt from `problem_statement` /
   `fail_to_pass` / `selected_test_files`, drives selfware via
   subprocess, captures the diff, and saves a `.pred` per `(quant,
   instance)`. Resumable via `--skip-existing`.
2. **Function calling at 256k context survives parallelism.** 4
   concurrent calculator-tool requests through llama-server's 2 slots
   all returned correct tool calls. q8_0 KV cache + `--parallel 2`
   keeps Q4_K_P at ~32 GB VRAM with 16 GB headroom.
3. **The progress-guard scaffold needs a SAB-vs-real-codebase split.**
   Today it triggers regardless of context.
4. **Single-trial variance dominates** (just like the SAB sweep). With
   the scaffold problem masking the model, we can't tell anything
   below it about quant capability on Pro tasks.

## Recommended next steps

1. **Fix the scaffold**, in priority order:
   - Skip the scaffold when the workdir already has source files
     (i.e. don't apply it to non-empty real repos).
   - When the scaffold does fire, look at the project's existing
     directory layout instead of hardcoding `src/lib.rs`.
   - Make the scaffold opt-in via config rather than always-on.
2. **Re-run this exact harness** after the fix; that gives us real
   per-quant numbers.
3. **Don't run the official SWE-bench Pro Docker eval yet** — applying
   these synthetic patches inside the test images will return 0% for
   every quant and waste a few hundred GB of pulls.

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
- `repo/` — the cloned + checked-out workdir
- `prompt.txt` — exactly what the agent saw
- `instance.json` — the full HF dataset row
- `agent.log` — full selfware stdout (very useful for diagnosing the scaffold)
- `<instance_id>.pred` — the captured diff (input to `helper_code/gather_patches.py`)
- `result.json` — exit code + wall time + patch size
