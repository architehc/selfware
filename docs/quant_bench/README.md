# Selfware quant benchmark — Qwen3.6 sweep, 2026-04-27

End-to-end benchmark of every Qwen3.6-27B HauhauCS quant + Qwen3.6-35B-A3B
on a single 2× RTX 4090 box, driven by the actual selfware agent. Each
quant runs 11 SAB-style coding scenarios (bug injected, agent given the
prompt, `cargo test` validates).

The full per-scenario breakdown is in
[`2026-04-27-qwen36-sweep-detail.md`](./2026-04-27-qwen36-sweep-detail.md).

## Headline ranking (by scenarios actually fixed)

| Rank | Quant | Pass | Speed (tok/s) | Size on disk |
|------|-------|------|--------------:|-------------:|
| 1 | **Qwen3.6-27B HauhauCS IQ4_XS** | **6 / 11** | 44.3 | 14 GB |
| 1 | **Qwen3.6-27B HauhauCS Q4_K_P** | **6 / 11** | 39.2 | 16 GB |
| 3 | Qwen3.6-27B HauhauCS Q2_K_P | 5 / 11 | 50.8 | 11 GB |
| 4 | Qwen3.6-35B-A3B-Q3_K_XL (MoE) | 4 / 11 | **125.0** | 16 GB |
| 4 | Qwen3.6-27B HauhauCS Q5_K_P | 4 / 11 | 34.3 | 20 GB |
| 6 | Qwen3.6-27B HauhauCS Q3_K_P | 3 / 11 | 43.9 | 14 GB |
| 7 | Qwen3.6-27B HauhauCS IQ2_M | 2 / 11 | 56.7 | 9 GB |
| 7 | Qwen3.6-27B HauhauCS IQ3_M | 2 / 11 | 48.8 | 12 GB |
| 7 | Qwen3.6-27B HauhauCS Q8_K_P | 2 / 11 | 24.0 | 30 GB |
| 10 | Qwen3.6-27B HauhauCS IQ3_XS | 1 / 11 | 52.0 | 12 GB |
| 10 | Qwen3.6-27B HauhauCS Q6_K_P | 1 / 11 | 31.6 | 22 GB |

## What's striking

1. **The 4-bit floor is the sweet spot.** IQ4_XS and Q4_K_P tie at the top
   (6/11) — both clearly above every higher-precision quant (Q5–Q8) in
   this run. "More bits" does not mean "better agent."

2. **Single-trial variance is huge on this scenario set.** Q6_K_P scored
   1/11 and Q8_K_P scored 2/11 — both worse than IQ2_M's 2/11. We're
   running each (quant × scenario) pair exactly once; the agent loop is
   stochastic enough that one bad early decision can wreck a run. To
   publish quality numbers we should re-run each pair 3-5× and report
   median. Treat this table as a **discriminator**, not a leaderboard.

3. **35B-A3B is the throughput pick.** 125 tok/s is 3× faster than the
   nearest 27B quant at 4/11 capability. For latency-sensitive flows
   (interactive chat, batch tool calls), it's the obvious default even if
   IQ4_XS / Q4_K_P fixes more scenarios per attempt.

4. **The synthetic 5/5 bench is dead.** The previous quant_benchmark
   reported 5/5 for *every* quant, including IQ2_M which actively
   fabricated "task complete" without editing files. The new harness ran
   every quant past `cargo test` — the spread (1/11 to 6/11) is real.

## How the bench works

- 11 scenarios drawn from `system_tests/projecte2e/templates/`. 4 use a
  `BugSpec::Patch` (find+replace one snippet to break a function); 7 ship
  pre-broken in the template (`BugSpec::None`).
- Per scenario:
  1. Copy template into a fresh work-dir.
  2. Inject the bug (or skip if pre-broken).
  3. Sanity check: validator (`cargo test`) must FAIL — otherwise the
     scenario isn't discriminating.
  4. Spawn the actual `selfware` binary against the work-dir (`-p` +
     `--yolo --no-tui --quiet`) with a 5-min timeout.
  5. Re-run the validator. Pass = pre-failed AND post-passed.
- Failure modes are recorded distinctly: `success` / `nonzero(N)` /
  `timeout` / `killed`, plus parsed step count from the agent's progress
  output.
- Work-dirs are kept on FAIL (under `/tmp/quant_bench_work/`) with a
  `_agent.log` containing the full agent stdout, so any "claimed
  complete but didn't fix" cases are inspectable.

## Reproducing

```bash
# 1. Detect what your hardware can run
cargo run --release --example quant_recommend

# 2. Download whichever quant the recommender picked
huggingface-cli download HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive \
    Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf \
    mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf \
    --local-dir ~/models/qwen36-quants/

# 3. Boot llama-server (pin the recommended thinking-off chat template)
llama-server -m ~/models/qwen36-quants/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf \
    --mmproj ~/models/qwen36-quants/mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf \
    --jinja -c 65536 -ngl 99 \
    --chat-template-kwargs '{"enable_thinking": false}' \
    --host 0.0.0.0 --port 8000

# 4. Run the bench
cargo run --release --example quant_benchmark -- \
    --endpoint http://127.0.0.1:8000/v1 \
    --quant my-quant \
    --output reports/quant_bench/my-quant.json \
    --model qwen3.6-27b

# Or sweep every .gguf in ~/models/qwen36-quants/ + 35B-A3B if present:
./scripts/quant_bench/run_full_sweep.sh
# (then: ./scripts/quant_bench/collate.py reports/quant_bench/<timestamp>/)
```

## What's next

- **3-5× retry per (quant × scenario)** to suppress single-run variance.
  At ~30 min/quant in single-trial, 3-trial median would push the sweep to
  ~18h — runnable overnight on a dedicated box, but worth a smaller
  scenario subset for routine benchmarking.
- **Deterministic seed** for the agent's sampling (where the model
  supports it via `extra_body.seed`), to remove one variance source
  before retries.
- **Progressive prompt difficulty**: same scenario, three prompt
  styles (terse / detailed / step-by-step) to separate "model can't"
  from "model wasn't told well."
- **Wire `selfware autoconfig --quant-recommend`**: today
  `quant_recommend.rs` is a standalone example. Fold it into the doctor
  flow so first-run users get a quant suggestion + download command
  printed automatically.
