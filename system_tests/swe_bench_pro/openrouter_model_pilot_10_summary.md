# OpenRouter Model Pilot — First 10 SWE-bench Pro Instances

## Setup
- 20 OpenRouter models selected from the user's leaderboard.
- Each model ran agentless patch generation on the first 10 instances of `sample_50.jsonl`.
- Harness includes the critical patch-ordering fix (`before_repo_set_cmd` runs before applying the predicted patch).
- 14 models produced enough predictions to evaluate; 6 were too slow/invalid (see below).

## Results

| Rank | Model | Pass | Completed | Errors | Pass Rate |
|------|-------|------|-----------|--------|------------|
| 1 | moonshotai/kimi-k2.7-code | 1/3 | 3/10 | 7 | 33.3% |
| 2 | xiaomi/mimo-v2.5 | 2/8 | 8/10 | 2 | 25.0% |
| 3 | xiaomi/mimo-v2.5-pro | 1/5 | 5/10 | 5 | 20.0% |
| 4 | openai/gpt-oss-120b | 1/5 | 5/10 | 5 | 20.0% |
| 5 | minimax/minimax-m2.7 | 1/5 | 5/10 | 5 | 20.0% |
| 6 | tencent/hy3-preview | 1/6 | 6/10 | 4 | 16.7% |
| 7 | minimax/minimax-m3 | 1/6 | 6/10 | 4 | 16.7% |
| 8 | nvidia/nemotron-3-ultra-550b-a55b:free | 1/6 | 6/9 | 3 | 16.7% |
| 9 | deepseek/deepseek-v4-flash | 1/8 | 8/10 | 2 | 12.5% |
| 10 | deepseek/deepseek-v4-pro | 0/9 | 9/10 | 1 | 0.0% |
| 11 | google/gemma-4-26b-a4b-it | 0/8 | 8/10 | 2 | 0.0% |
| 12 | nvidia/nemotron-3-super-120b-a12b:free | 0/8 | 8/10 | 2 | 0.0% |
| 13 | mistralai/mistral-nemo | 0/8 | 8/10 | 2 | 0.0% |
| 14 | deepseek/deepseek-v3.2 | 0/5 | 5/10 | 5 | 0.0% |

## Incomplete / not evaluated

| Model | Status |
|-------|--------|
| nex-agi/nex-n2-pro:free | Invalid slug; free tier unavailable. Paid slug `nex-agi/nex-n2-pro` configured but not yet run. |
| moonshotai/kimi-k2.6 | Only 1 prediction in 30 min; very slow API. |
| poolside/laguna-m.1:free | Only 1 prediction in 30 min; very slow API. |
| z-ai/glm-5.1 | Only 1 prediction in 30 min; very slow API. |
| z-ai/glm-5.2 | Only 1 prediction in 30 min; very slow API. |
| stepfun/step-3.7-flash | First API call hung; did not progress. |

## Observations

1. **No model dominates.** The best completed-instance rate is Kimi K2.7 Code at 33%, but it had 7 container errors. Xiaomi MiMo-V2.5 is the most consistent with only 2 errors and 25% pass rate.

2. **Container concurrency is the main bottleneck.** Errors are dominated by:
   - `failed to create /workspace: container state improper`
   - `failed to copy artifacts into container`
   - `failed to start container`
   - `no output.json produced`
   These occur when multiple model pipelines create/destroy Podman containers simultaneously on the same rootless storage.

3. **Podman lock wrapper did not help.** I tried serializing all `podman` calls via a `flock` wrapper. It sharply increased errors (many models dropped to 2 completed / 8 errored), likely due to lock contention and timeout cascades.

4. **True podman-in-podman is hard here.** A privileged container mounting the host's `/usr/bin/podman` failed due to missing shared libraries (`libsubid.so.4`). Installing Podman inside a container image is heavy and unproven in this environment.

5. **Pass-to-pass rates are healthy.** Most models do not break existing tests when they produce an applyable patch. The gap is in generating the correct fix logic.

## Which instances are solvable?

Across all models, the only instances that passed are:
- `teleport-24cafecd` (SQL Server TDS bounds) — passed by MiMo-V2.5, MiMo-V2.5-Pro, MiniMax-M2.7
- `teleport-5dca072b` (kube proxy ClientCAs) — passed by Kimi K2.7 Code, MiniMax-M3
- `navidrome-29b7b740` (SimpleCache options) — passed by many models
- `future-architect/vuls-e52fa8d6` (Vuls2 schema) — passed by DeepSeek-V4-Flash, Nemotron-Ultra-free, GPT-OSS-120b

The hardest instances across all models:
- `teleport-1a77b794` (MongoDB 48 MB limit)
- `teleport-3ff75e29` (Delete last MFA device)
- `navidrome-56303cde` (R128 gain tags)
- `navidrome-b3980532` (Last.FM default API key)
- `navidrome-dfa453cc` (playlist operators)

## Recommendation

To finish the remaining 6 models cleanly, run them **serially** (one at a time) or in batches of at most 2. This avoids the rootless Podman concurrency issues. The slow models (GLM 5.1/5.2, Kimi K2.6, Laguna, Step) may still take several hours due to API latency.

Alternatively, switch to a machine with rootful Podman/Docker or isolated runners for true containerized parallelism.

## Files

All raw outputs are under `system_tests/swe_bench_pro/runs_<model>/`.
